use crate::route_prelude::*;

#[derive(Template)]
#[template(path = "tag.html")]
struct RepoTagTemplate<'a> {
  repo: &'a Repository,
  tag: Tag<'a>,
}

pub(crate) async fn repo_tag(req: Request<()>) -> tide::Result {
  let repo = repo_from_request(req.param("repo_name")?)?;
  let tag = repo.revparse_single(req.param("tag")?)?.peel_to_tag();

  if let Ok(tag) = tag {
    let tmpl = RepoTagTemplate { repo: &repo, tag };
    Ok(tmpl.into())
  } else {
    Ok(
      tide::Redirect::permanent(format!(
        "/{}/commit/{}",
        req.param("repo_name")?,
        req.param("tag")?
      ))
      .into(),
    )
  }
}
