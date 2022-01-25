use crate::route_prelude::*;

lazy_static! {
  static ref CODE_REGEX: Regex =
    Regex::new("<code class=\".*?language-(?P<language>.*?)*?\">(?P<code>(?s).*?)</code>").unwrap();
}

#[derive(Template)]
#[template(path = "repo.html")] // using the template in this path, relative
struct RepoHomeTemplate<'a> {
  repo: &'a Repository,
  commits: Vec<Commit<'a>>,
  readme_text: String,
}

pub(crate) async fn repo_home(req: Request<()>) -> tide::Result {
  use pulldown_cmark::{escape::escape_html, html::push_html, Options, Parser};

  enum ReadmeFormat {
    Plaintext,
    Html,
    Markdown,
  }

  let repo = repo_from_request(req.param("repo_name")?)?;

  let mut format = ReadmeFormat::Plaintext;
  let mut readme_text = repo
    .revparse_single("HEAD:README")
    .or_else(|_| repo.revparse_single("HEAD:README.txt"))
    .or_else(|_| {
      format = ReadmeFormat::Markdown;
      repo.revparse_single("HEAD:README.md")
    })
    .or_else(|_| repo.revparse_single("HEAD:README.mdown"))
    .or_else(|_| repo.revparse_single("HEAD:README.markdown"))
    .or_else(|_| {
      format = ReadmeFormat::Html;
      repo.revparse_single("HEAD:README.html")
    })
    .or_else(|_| repo.revparse_single("HEAD:README.htm"))
    .ok()
    .and_then(|readme| readme.into_blob().ok())
    .map(|blob| {
      let text = std::str::from_utf8(blob.content()).unwrap_or_default();

      // render the file contents to HTML
      match format {
        // render plaintext as preformatted text
        ReadmeFormat::Plaintext => {
          let mut output = "<pre>".to_string();
          escape_html(&mut output, text).unwrap();
          output.push_str("</pre>");
          output
        }
        // already is HTML
        ReadmeFormat::Html => text.into(),
        // render Markdown to HTML
        ReadmeFormat::Markdown => {
          let parser = Parser::new_ext(text, Options::all());

          let mut output = String::new();
          push_html(&mut output, parser);
          output
        }
      }
    })
    .unwrap_or_default()
    .replace("&quot;", "\"");

  // replace code in markdown with syntax higlighted code
  for capture in CODE_REGEX.captures_iter(
    #[allow(clippy::redundant_clone, reason = "its not a redundant clone")]
    &readme_text.clone(),
  ) {
    let syntax = SYNTAXES
      .find_syntax_by_token(&capture["language"])
      .unwrap_or_else(|| SYNTAXES.find_syntax_plain_text());

    let mut highlighter =
      ClassedHTMLGenerator::new_with_class_style(syntax, &SYNTAXES, ClassStyle::Spaced);

    LinesWithEndings::from(&capture["code"])
      .for_each(|line| highlighter.parse_html_for_line_which_includes_newline(line));

    readme_text = readme_text.replace(
      &format!("<pre>{}</pre>", &capture[0]),
      &highlighter.finalize(),
    );
  }

  println!("{}", readme_text);

  // get the first few commits for a preview
  let commits = if repo.is_shallow() {
    tide::log::warn!("Repository {:?} is only a shallow clone", repo.path());
    vec![repo.head()?.peel_to_commit().unwrap()]
  } else {
    let mut revwalk = repo.revwalk()?;
    let r = req.param("ref").unwrap_or("HEAD");
    revwalk.push(repo.revparse_single(r)?.peel_to_commit()?.id())?;

    revwalk.set_sorting(git2::Sort::TIME).unwrap();
    revwalk
      .filter_map(|oid| repo.find_commit(oid.ok()?).ok())
      .take(3)
      .collect()
  };

  Ok(
    RepoHomeTemplate {
      repo: &repo,
      commits,
      readme_text,
    }
    .into(),
  )
}
