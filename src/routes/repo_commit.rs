use crate::route_prelude::*;

#[derive(Template)]
#[template(path = "commit.html")] // using the template in this path, relative
struct RepoCommitTemplate<'a> {
  repo: &'a Repository,
  commit: Commit<'a>,
  diff: &'a Diff<'a>
}

impl RepoCommitTemplate<'_> {
  fn parent_ids(&self) -> Vec<git2::Oid> {
    self.commit.parent_ids().collect()
  }

  fn diff(&self) -> String {
    let mut buf = String::new();
    self
      .diff
      .print(
        git2::DiffFormat::Patch,
        |_delta, _hunk, line| match str::from_utf8(line.content()) {
          Ok(content) => {
            match line.origin() {
              'F' | 'H' => {}
              c @ ' ' | c @ '+' | c @ '-' | c @ '=' | c @ '<' | c @ '>' => buf.push(c),
              _ => unreachable!()
            }
            buf.push_str(content);
            true
          }
          Err(_) => {
            buf.push_str("Cannot display diff for binary file.");
            false
          }
        }
      )
      .unwrap();

    // highlight the diff
    let syntax = SYNTAXES
      .find_syntax_by_name("Diff")
      .expect("diff syntax missing");
    let mut highlighter =
      ClassedHTMLGenerator::new_with_class_style(syntax, &SYNTAXES, ClassStyle::Spaced);
    LinesWithEndings::from(&buf)
      .for_each(|line| highlighter.parse_html_for_line_which_includes_newline(line));
    highlighter.finalize()
  }

  fn refs(&self) -> String {
    use git2::{BranchType, DescribeFormatOptions, DescribeOptions};

    let mut html = String::new();

    // add badge if this commit is a tag
    let descr = self.commit.as_object().describe(
      DescribeOptions::new()
        .describe_tags()
        .max_candidates_tags(0)
    );
    if let Ok(descr) = descr {
      // this can be a tag or lightweight tag, the refs path will redirect
      html += &format!(
        r#"<a href="/{0}/refs/{1}" class="badge tag">{1}</a>"#,
        filters::repo_name(self.repo).unwrap(),
        descr
          .format(Some(DescribeFormatOptions::new().abbreviated_size(0)))
          .unwrap(),
      );
    }

    // also add badge if this is the tip of a branch
    let branches = self
      .repo
      .branches(Some(BranchType::Local))
      .unwrap()
      .filter_map(|x| if let Ok(x) = x { Some(x.0) } else { None })
      .filter(|branch| branch.get().peel_to_commit().unwrap().id() == self.commit.id());
    for branch in branches {
      // branch is not a reference, just a fancy name for a commit
      html += &format!(
        r#" <a href="/{0}/log/{1}" class="badge branch">{1}</a>"#,
        filters::repo_name(self.repo).unwrap(),
        branch.name().unwrap().unwrap(),
      );
    }

    html
  }
}

pub(crate) async fn repo_commit(req: Request<()>) -> tide::Result {
  let repo = repo_from_request(req.param("repo_name")?)?;
  let commit = repo
    .revparse_single(req.param("commit")?)?
    .peel_to_commit()?;

  // This is identical to getting "commit^" and on merges this will be the
  // merged into branch before the merge.
  let parent_tree = commit.parent(0).ok().map(|parent| parent.tree().unwrap());

  let mut diff = repo.diff_tree_to_tree(parent_tree.as_ref(), Some(&commit.tree()?), None)?;
  let mut find_options = git2::DiffFindOptions::new();
  // try to find moved/renamed files
  find_options.all(true);
  diff.find_similar(Some(&mut find_options)).unwrap();

  let tmpl = RepoCommitTemplate {
    repo: &repo,
    commit,
    diff: &diff
  };
  Ok(tmpl.into())
}
