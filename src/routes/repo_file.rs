use crate::route_prelude::*;

#[derive(Template)]
#[template(path = "file.html")] // using the template in this path, relative
struct RepoFileTemplate<'a> {
  repo: &'a Repository,
  path: &'a Path,
  file_text: &'a str,
  spec: &'a str,
  last_commit: Commit<'a>
}

pub(crate) async fn repo_file(req: Request<()>) -> tide::Result {
  let repo = repo_from_request(req.param("repo_name")?)?;

  if repo.is_empty().unwrap() {
    // redirect to start page of repo
    let mut url = req.url().clone();
    url.path_segments_mut().unwrap().pop();
    return Ok(tide::Redirect::temporary(url.to_string()).into());
  }

  let head = repo.head()?;
  let spec = req.param("ref").ok().or_else(|| head.shorthand()).unwrap();
  let commit = repo.revparse_single(spec)?.peel_to_commit()?;
  let tree = commit.tree()?;

  let (path, tree_obj) = if let Ok(path) = req.param("object_name") {
    let path = Path::new(path);
    (path, tree.get_path(path)?.to_object(&repo)?)
  } else {
    (Path::new(""), tree.into_object())
  };

  let last_commit = crate::last_commit_for(&repo, spec, path);

  // TODO make sure I am escaping html properly here
  // TODO allow disabling of syntax highlighting
  // TODO -- dont pull in memory, use iterators if possible
  let tmpl = match tree_obj.into_tree() {
    // this is a subtree
    Ok(tree) => crate::RepoTreeTemplate {
      repo: &repo,
      tree,
      path,
      spec,
      last_commit
    }
    .into(),
    // this is not a subtree, so it should be a blob i.e. file
    Err(tree_obj) => {
      let extension = path
        .extension()
        .and_then(std::ffi::OsStr::to_str)
        .unwrap_or_default();
      let syntax = SYNTAXES
        .find_syntax_by_extension(extension)
        .unwrap_or_else(|| SYNTAXES.find_syntax_plain_text());

      let blob = tree_obj.as_blob().unwrap();
      let output = if blob.is_binary() {
        // this is not a text file, but try to serve the file if the MIME type
        // can give a hint at how
        let mime = http::Mime::from_extension(extension).unwrap_or_else(|| {
          if blob.is_binary() {
            http::mime::BYTE_STREAM
          } else {
            http::mime::PLAIN
          }
        });
        match mime.basetype() {
                    "text" => unreachable!("git detected this file as binary"),
                    "image" => format!(
                        "<img src=\"/{}/tree/{spec}/raw/{}\" />",
                        req.param("repo_name").unwrap(),
                        path.display()
                    ),
                    tag@"audio"|tag@"video" => format!(
                        "<{} src=\"/{}/tree/{spec}/raw/{}\" controls>Your browser does not have support for playing this {0} file.</{0}>",
                        tag,
                        req.param("repo_name").unwrap(),
                        path.display()
                    ),
                    _ => "Cannot display binary file.".into()
                }
      } else {
        // get file contents from git object
        let file_string = std::str::from_utf8(tree_obj.as_blob().unwrap().content())?;
        // create a highlighter that uses CSS classes so we can use prefers-color-scheme
        let mut highlighter =
          ClassedHTMLGenerator::new_with_class_style(syntax, &SYNTAXES, ClassStyle::Spaced);
        LinesWithEndings::from(file_string)
          .for_each(|line| highlighter.parse_html_for_line_which_includes_newline(line));

        // use oid so it is a permalink
        let prefix = format!(
          "/{}/tree/{}/item/{}",
          req.param("repo_name").unwrap(),
          commit.id(),
          path.display()
        );

        let mut output = String::from("<pre>\n");
        for (n, line) in highlighter.finalize().lines().enumerate() {
          output.push_str(&format!(
            "<a href='{prefix}#L{0}' id='L{0}' class='line'>{0}</a>{line}\n",
            n + 1,
          ));
        }
        output.push_str("</pre>\n");
        output
      };
      RepoFileTemplate {
        repo: &repo,
        path,
        file_text: &output,
        spec,
        last_commit
      }
      .into()
    }
  };
  Ok(tmpl)
}

pub async fn repo_file_raw(req: Request<()>) -> tide::Result {
  let repo = repo_from_request(req.param("repo_name")?)?;

  let spec = req.param("ref").unwrap();
  let tree = repo.revparse_single(spec)?.peel_to_commit()?.tree()?;

  let path = Path::new(req.param("object_name")?);
  let blob = tree
    .get_path(path)
    .and_then(|tree_entry| tree_entry.to_object(&repo)?.peel_to_blob());
  match blob {
    Ok(blob) => {
      let extension = path
        .extension()
        .and_then(std::ffi::OsStr::to_str)
        .unwrap_or_default();
      let mime = http::Mime::from_extension(extension).unwrap_or_else(|| {
        if blob.is_binary() {
          http::mime::BYTE_STREAM
        } else {
          http::mime::PLAIN
        }
      });

      // have to put the blob's content into a Vec here because the repo will be dropped
      Ok(
        Response::builder(200)
          .body(blob.content().to_vec())
          .content_type(mime)
          .build()
      )
    }
    Err(e) => Err(tide::Error::from_str(
      404,
      format!(
        "There is no such file in this revision of the repository: {}",
        e
      )
    ))
  }
}
