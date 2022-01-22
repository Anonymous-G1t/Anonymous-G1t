use crate::route_prelude::*;

#[derive(Template)]
#[template(path = "index.html")] // using the template in this path, relative
struct IndexTemplate {
  repos: Vec<Repository>
}

pub(crate) async fn index(req: Request<()>) -> tide::Result {
  // check for gitweb parameters to redirect
  if let Some(query) = req.url().query() {
    // gitweb does not use standard & separated query parameters
    let query = query
      .split(';')
      .map(|s| {
        let mut parts = s.splitn(2, '=');
        (parts.next().unwrap(), parts.next().unwrap())
      })
      .collect::<std::collections::HashMap<_, _>>();
    if let Some(repo) = query.get("p") {
      return Ok(
        tide::Redirect::permanent(match query.get("a") {
          None | Some(&"summary") => format!("/{}/", repo),
          Some(&"commit") | Some(&"commitdiff") => {
            format!("/{}/commit/{}", repo, query.get("h").cloned().unwrap_or(""))
          }
          Some(&"shortlog") | Some(&"log") => {
            format!("/{}/log/{}", repo, query.get("h").cloned().unwrap_or(""))
          }
          Some(_) => "/".to_string()
        })
        .into()
      );
    }
  }

  let repos = fs::read_dir(&CONFIG.repos_root)
    .map(|entries| {
      entries
        .filter_map(|entry| Some(entry.ok()?.path()))
        .filter_map(|entry| Repository::open(entry).ok())
        .filter(|repo| {
          // check for the export file in the git directory
          // (the .git subfolder for non-bare repos)
          repo.path().join(&CONFIG.export_ok).exists()
        })
        .collect::<Vec<_>>()
    })
    .map_err(|e| tide::log::warn!("Can't read repositories: {}", e))
    .unwrap_or_default();
  let index_template = IndexTemplate { repos };

  Ok(index_template.into())
}
