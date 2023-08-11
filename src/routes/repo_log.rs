use crate::route_prelude::*;

#[derive(Template)]
#[template(path = "log.html")] // using the template in this path, relative
struct RepoLogTemplate<'a> {
  repo: &'a Repository,
  commits: Vec<Commit<'a>>,
  branch: &'a str,
  // the spec the user should be linked to to see the next page of commits
  next_page: Option<String>,
}

pub(crate) async fn repo_log(req: Request<()>) -> tide::Result {
  let repo = repo_from_request(req.param("repo_name")?)?;
  if repo.is_empty().unwrap() {
    // redirect to start page of repo
    let mut url = req.url().clone();
    url.path_segments_mut().unwrap().pop();
    return Ok(tide::Redirect::temporary(url).into());
  }

  let next_page_spec;
  let mut commits = if repo.is_shallow() {
    tide::log::warn!("repository {:?} is only a shallow clone", repo.path());
    next_page_spec = "".into();
    vec![repo.head()?.peel_to_commit().unwrap()]
  } else {
    let mut revwalk = repo.revwalk()?;
    let r = req.param("ref").unwrap_or("HEAD");
    revwalk.push(repo.revparse_single(r)?.peel_to_commit()?.id())?;

    if let Some(i) = r.rfind('~') {
      // there is a tilde, try to find a number too
      let n = r[i + 1..].parse::<usize>().ok().unwrap_or(1);
      next_page_spec = format!("{}~{}", &r[..i], n + CONFIG.log_per_page);
    } else {
      // there was no tilde
      next_page_spec = format!("{}~{}", r, CONFIG.log_per_page);
    }

    revwalk.set_sorting(git2::Sort::TIME).unwrap();
    let commits = revwalk.filter_map(|oid| repo.find_commit(oid.unwrap()).ok()); // TODO error handling

    // filter for specific file if necessary
    if let Ok(path) = req.param("object_name") {
      let mut options = DiffOptions::new();
      options.pathspec(path);
      commits
        .filter(|commit|
                // check that the given file was affected from any of the parents
                commit.parents().any(|parent|
                    repo.diff_tree_to_tree(
                        Some(&commit.tree().unwrap()),
                        Some(&parent.tree().unwrap()),
                        Some(&mut options),
                    ).unwrap().stats().unwrap().files_changed()>0
                ))
        .take(CONFIG.log_per_page + 1)
        .collect()
    } else {
      commits.take(CONFIG.log_per_page + 1).collect()
    }
  };

  // check if there even is a next page
  let next_page = if commits.len() < CONFIG.log_per_page + 1 {
    None
  } else {
    // remove additional commit from next page check
    commits.pop();
    Some(next_page_spec)
  };

  let head_branch = repo.head()?;
  let branch = req
    .param("ref")
    .ok()
    .or_else(|| head_branch.shorthand())
    .unwrap();
  let tmpl = RepoLogTemplate {
    repo: &repo,
    commits,
    branch,
    next_page,
  };
  Ok(tmpl.into())
}
