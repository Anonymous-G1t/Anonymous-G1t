#![allow(clippy::from_over_into)]
use askama::Template;
use git2::{Commit, DiffOptions, Repository, Tree};
use once_cell::sync::Lazy;
use serde::Deserialize;
use std::fs;
use std::path::Path;
use std::str;
use syntect::parsing::SyntaxSet;

use tide::Request;

pub(crate) mod errorpage;
pub(crate) mod filters;
pub(crate) mod routes;

#[derive(Deserialize, Debug)]
pub(crate) struct Config {
  #[serde(default = "defaults::port")]
  port: u16,
  #[serde(default = "defaults::repo_directory")]
  repos_root: String,
  #[serde(default = "String::new")]
  emoji_favicon: String,
  #[serde(default = "defaults::site_name")]
  site_name: String,
  #[serde(default = "defaults::export_ok")]
  export_ok: String,
  #[serde(default = "String::new")]
  clone_base: String,
  #[serde(default = "defaults::log_per_page")]
  log_per_page: usize,
}

/// Defaults for the configuration options
// FIXME: simplify if https://github.com/serde-rs/serde/issues/368 is resolved
mod defaults {
  pub(crate) fn port() -> u16 {
    80
  }

  pub(crate) fn repo_directory() -> String {
    "repos".into()
  }

  pub(crate) fn site_name() -> String {
    "agit".into()
  }

  pub(crate) fn export_ok() -> String {
    "git-daemon-export-ok".into()
  }

  pub(crate) fn log_per_page() -> usize {
    100
  }
}

const HELP: &str = "
Usage: agit

FLAGS:
  -h, --help            Prints this help information and exits.
OPTIONS:
  -c, --config <FILE>   Use a specific configuration file.
                        default is ./agit.toml
";

pub(crate) static CONFIG: Lazy<Config> = Lazy::new(args);

// so we only have to load this once to reduce startup time for syntax highlighting
pub(crate) static SYNTAXES: Lazy<SyntaxSet> = Lazy::new(|| {
  let syntaxes = Path::new("syntaxes");

  if syntaxes.exists() {
    let mut builder = SyntaxSet::load_defaults_newlines().into_builder();

    builder.add_from_folder(syntaxes, true).unwrap();

    builder.build()
  } else {
    SyntaxSet::load_defaults_newlines()
  }
});

#[derive(rust_embed::RustEmbed)]
#[folder = "$CARGO_MANIFEST_DIR/templates/static"]
struct StaticDir;

fn args() -> Config {
  // TODO cli

  let mut pargs = pico_args::Arguments::from_env();

  if pargs.contains(["-h", "--help"]) {
    print!("{}", HELP);
    std::process::exit(0);
  }

  let config_filename = pargs
    .opt_value_from_str(["-c", "--config"])
    .unwrap()
    .unwrap_or_else(|| "agit.toml".to_string());

  let toml_text = fs::read_to_string(&config_filename).unwrap_or_else(|_| {
    tide::log::warn!(
      "Configuration file {:?} not found, using defaults",
      config_filename
    );
    String::new()
  });
  match toml::from_str(&toml_text) {
    Ok(config) => config,
    Err(e) => {
      eprintln!("could not parse configuration file: {}", e);
      std::process::exit(1);
    }
  }
}

pub(crate) fn repo_from_request(repo_name: &str) -> Result<Repository, tide::Error> {
  let repo_name = percent_encoding::percent_decode_str(repo_name)
    .decode_utf8_lossy()
    .into_owned();

  let repo_path = Path::new(&CONFIG.repos_root)
    .join(repo_name)
    .canonicalize()?;

  Repository::open(repo_path)
    .ok()
    // outside users should not be able to tell the difference between
    // nonexistent and existing but forbidden repos, so not using 403
    .filter(|repo| repo.path().join(&CONFIG.export_ok).exists())
    .ok_or_else(|| tide::Error::from_str(404, "This repository does not exist."))
}

fn last_commit_for<'a, S: git2::IntoCString>(
  repo: &'a Repository,
  spec: &str,
  path: S,
) -> Commit<'a> {
  let mut revwalk = repo.revwalk().unwrap();
  revwalk
    .push(
      repo
        // we already know this has to be a commit-ish
        .revparse_single(spec)
        .unwrap()
        .peel_to_commit()
        .unwrap()
        .id(),
    )
    .unwrap();
  revwalk.set_sorting(git2::Sort::TIME).unwrap();

  let mut options = DiffOptions::new();
  options.pathspec(path);

  revwalk
    .filter_map(|oid| repo.find_commit(oid.unwrap()).ok()) // TODO error handling
    .find(|commit| {
      let tree = commit.tree().unwrap();
      if commit.parent_count() == 0 {
        repo
          .diff_tree_to_tree(None, Some(&tree), Some(&mut options))
          .unwrap()
          .stats()
          .unwrap()
          .files_changed()
          > 0
      } else {
        // check that the given file was affected from any of the parents
        commit.parents().any(|parent| {
          repo
            .diff_tree_to_tree(parent.tree().ok().as_ref(), Some(&tree), Some(&mut options))
            .unwrap()
            .stats()
            .unwrap()
            .files_changed()
            > 0
        })
      }
    })
    .expect("file was not part of any commit")
}

#[derive(Template)]
#[template(path = "tree.html")] // using the template in this path, relative
struct RepoTreeTemplate<'a> {
  repo: &'a Repository,
  tree: Tree<'a>,
  path: &'a Path,
  spec: &'a str,
  last_commit: Commit<'a>,
}

async fn git_data(req: Request<()>) -> tide::Result {
  let repo = repo_from_request(req.param("repo_name")?)?;
  let path = req
    .url()
    .path()
    .strip_prefix(&format!("/{}/", req.param("repo_name").unwrap()))
    .unwrap_or_default();
  let path = repo.path().join(path).canonicalize()?;

  if !path.starts_with(repo.path()) {
    // that path got us outside of the repository structure somehow
    tide::log::warn!("Attempt to acces file outside of repo dir: {:?}", path);
    Err(tide::Error::from_str(
      403,
      "You do not have access to this file.",
    ))
  } else if !path.is_file() {
    // Either the requested resource does not exist or it is not
    // a file, i.e. a directory.
    Err(tide::Error::from_str(404, "This page does not exist."))
  } else {
    // ok - inside the repo directory
    let mut resp = tide::Response::new(200);
    let mut body = tide::Body::from_file(path).await?;
    body.set_mime("text/plain; charset=utf-8");
    resp.set_body(body);
    Ok(resp)
  }
}

#[async_std::main]
async fn main() -> Result<(), std::io::Error> {
  fs::create_dir_all(CONFIG.repos_root.clone()).ok();

  femme::start(femme::Logger::Pretty);

  tide::log::info!("Please report bugs at https://github.com/TheBotlyNoob/agit\n");

  let mut app = tide::new();

  app.with(errorpage::ErrorToErrorpage);

  app.at("/").get(routes::index);

  // repositories
  app
    .at("/:repo_name")
    .get(routes::repo_home)
    .at("/:repo_name/")
    .get(routes::repo_home);

  // git clone stuff
  app
    .at("/:repo_name/info/refs")
    .get(git_data)
    .at("/:repo_name/HEAD")
    .get(git_data)
    .at("/:repo_name/objects/*obj")
    .get(git_data);

  // web pages
  app
    .at("/:repo_name/commit/:commit")
    .get(routes::repo_commit)
    .at("/:repo_name/refs")
    .get(routes::repo_refs)
    .at("/:repo_name/refs/")
    .get(routes::repo_refs)
    .at("/:repo_name/refs.xml")
    .get(routes::repo_refs_feed)
    .at("/:repo_name/refs/:tag")
    .get(routes::repo_tag);

  app
    .at("/:repo_name/log")
    .get(routes::repo_log)
    .at("/:repo_name/log/")
    .get(routes::repo_log)
    .at("/:repo_name/log/:ref")
    .get(routes::repo_log) // ref is optional
    .at("/:repo_name/log/:ref/")
    .get(routes::repo_log) // ref is optional
    .at(&format!(
      "/{}/:repo_name/log/:ref/*object_name",
      CONFIG.repos_root
    ))
    .get(routes::repo_log)
    .at("/:repo_name/log.xml")
    .get(routes::repo_log_feed)
    .at(&format!(
      "/{}/:repo_name/log/:ref/feed.xml",
      CONFIG.repos_root
    ))
    .get(routes::repo_log_feed); // ref is optional

  app
    .at("/:repo_name/tree")
    .get(routes::repo_file)
    .at("/:repo_name/tree/")
    .get(routes::repo_file)
    .at("/:repo_name/tree/:ref")
    .get(routes::repo_file) // ref is optional
    .at("/:repo_name/tree/:ref/")
    .get(routes::repo_file); // ref is optional

  app
    .at("/:repo_name/tree/:ref/item/*object_name")
    .get(routes::repo_file);

  app
    .at("/:repo_name/tree/:ref/raw/*object_name")
    .get(routes::repo_file_raw);

  // static files
  app.at("/static/*path").all(routes::static_resource);

  app.listen(format!("0.0.0.0:{}", CONFIG.port)).await?;

  Ok(())
}

pub(crate) mod route_prelude {
  pub(crate) use crate::{filters, repo_from_request, StaticDir, CONFIG, SYNTAXES};
  pub(crate) use askama::Template;
  pub(crate) use git2::{Commit, Diff, DiffOptions, Reference, Repository, Signature, Tag};
  pub(crate) use once_cell::sync::Lazy;
  pub(crate) use regex::Regex;
  pub(crate) use std::{fs, path::Path, str};
  pub(crate) use syntect::{
    html::{ClassStyle, ClassedHTMLGenerator},
    util::LinesWithEndings,
  };
  pub(crate) use tide::{http, Request, Response};
}
