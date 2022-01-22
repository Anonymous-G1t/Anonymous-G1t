use crate::route_prelude::*;

#[derive(Template)]
#[template(path = "refs.html")] // using the template in this path, relative
struct RepoRefTemplate<'a> {
  repo: &'a Repository,
  branches: Vec<Reference<'a>>,
  tags: Vec<(String, String, Signature<'static>)>
}

pub(crate) async fn repo_refs(req: Request<()>) -> tide::Result {
  let repo = repo_from_request(req.param("repo_name")?)?;
  if repo.is_empty().unwrap() {
    // redirect to start page of repo
    let mut url = req.url().clone();
    url.path_segments_mut().unwrap().pop();
    return Ok(tide::Redirect::temporary(url.to_string()).into());
  }

  let branches = repo
    .references()?
    .filter_map(|x| x.ok())
    .filter(|x| x.is_branch())
    .collect();
  let mut tags = Vec::new();
  repo
    .tag_foreach(|oid, name_bytes| {
      // remove prefix "ref/tags/"
      let name = String::from_utf8(name_bytes[10..].to_vec()).unwrap();

      let obj = repo.find_object(oid, None).unwrap();
      tags.push(match obj.kind().unwrap() {
        git2::ObjectType::Tag => (
          format!("refs/{}", name),
          name,
          obj
            .as_tag()
            .unwrap()
            .tagger()
            .unwrap_or_else(|| obj.peel_to_commit().unwrap().committer().to_owned())
            .to_owned()
        ),
        git2::ObjectType::Commit => {
          // lightweight tag
          (
            format!("commit/{}", name),
            name,
            obj.as_commit().unwrap().committer().to_owned()
          )
        }
        _ => unreachable!("a tag was not a tag or lightweight tag")
      });
      true
    })
    .unwrap();
  // sort so that newest tags are at the top
  tags.sort_unstable_by(|(_, _, a), (_, _, b)| a.when().cmp(&b.when()).reverse());
  let tmpl = RepoRefTemplate {
    repo: &repo,
    branches,
    tags
  };
  Ok(tmpl.into())
}
