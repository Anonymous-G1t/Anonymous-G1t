use crate::route_prelude::*;

#[derive(Template)]
#[template(path = "log.xml")]
struct RepoLogFeedTemplate<'a> {
  repo: &'a Repository,
  commits: Vec<Commit<'a>>,
  branch: &'a str,
  base_url: &'a str,
}

pub(crate) async fn repo_log_feed(req: Request<()>) -> tide::Result {
  let repo = repo_from_request(req.param("repo_name")?)?;
  if repo.is_empty().unwrap() {
    // show a server error
    return Err(tide::Error::from_str(
      503,
      "Cannot show feed because there are no commits.",
    ));
  }

  let commits = if repo.is_shallow() {
    tide::log::warn!("repository {:?} is only a shallow clone", repo.path());
    vec![repo.head()?.peel_to_commit().unwrap()]
  } else {
    let mut revwalk = repo.revwalk()?;
    let r = req.param("ref").unwrap_or("HEAD");
    revwalk.push(repo.revparse_single(r)?.peel_to_commit()?.id())?;

    revwalk.set_sorting(git2::Sort::TIME).unwrap();
    revwalk
      .filter_map(|oid| repo.find_commit(oid.unwrap()).ok()) // TODO error handling
      .take(CONFIG.log_per_page)
      .collect()
  };

  let head_branch = repo.head()?;
  let branch = req
    .param("ref")
    .ok()
    .or_else(|| head_branch.shorthand())
    .unwrap();

  let mut url = req.url().clone();
  {
    let mut segments = url.path_segments_mut().unwrap();
    segments.pop(); // pop "log.xml" or "feed.xml"
    if req.param("ref").is_ok() {
      segments.pop(); // pop ref
      segments.pop(); // pop "log/"
    }
  }

  let tmpl = RepoLogFeedTemplate {
    repo: &repo,
    commits,
    branch,
    base_url: url.as_str(),
  };
  let mut response: tide::Response = tmpl.into();
  response.set_content_type("application/rss+xml");
  Ok(response)
}
