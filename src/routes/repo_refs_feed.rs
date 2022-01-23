use crate::route_prelude::*;

#[derive(Template)]
#[template(path = "refs.xml")]
struct RepoRefFeedTemplate<'a> {
  repo: &'a Repository,
  tags: Vec<(String, String, Signature<'static>, String)>,
  base_url: &'a str,
}

pub(crate) async fn repo_refs_feed(req: Request<()>) -> tide::Result {
  let repo = repo_from_request(req.param("repo_name")?)?;
  if repo.is_empty().unwrap() {
    // show a server error
    return Err(tide::Error::from_str(
      503,
      "Cannot show feed because there is nothing here.",
    ));
  }

  let mut tags = Vec::new();
  repo
    .tag_foreach(|oid, name_bytes| {
      // remove prefix "ref/tags/"
      let name = String::from_utf8(name_bytes[10..].to_vec()).unwrap();

      let obj = repo.find_object(oid, None).unwrap();
      tags.push(match obj.kind().unwrap() {
        git2::ObjectType::Tag => {
          let tag = obj.as_tag().unwrap();
          (
            format!("refs/{}", name),
            name,
            tag
              .tagger()
              .unwrap_or_else(|| obj.peel_to_commit().unwrap().committer().to_owned())
              .to_owned(),
            tag.message().unwrap_or("").into(),
          )
        }
        git2::ObjectType::Commit => {
          // lightweight tag, therefore no content
          (
            format!("commit/{}", name),
            name,
            obj.as_commit().unwrap().committer().to_owned(),
            String::new(),
          )
        }
        _ => unreachable!("a tag was not a tag or lightweight tag"),
      });
      true
    })
    .unwrap();
  // sort so that newest tags are at the top
  tags.sort_unstable_by(|(_, _, a, _), (_, _, b, _)| a.when().cmp(&b.when()).reverse());

  let mut url = req.url().clone();
  {
    let mut segments = url.path_segments_mut().unwrap();
    segments.pop(); // pop "log.xml" or "feed.xml"
    if req.param("ref").is_ok() {
      segments.pop(); // pop ref
      segments.pop(); // pop "log/"
    }
  }

  let tmpl = RepoRefFeedTemplate {
    repo: &repo,
    tags,
    base_url: url.as_str(),
  };

  Ok(tmpl.into())
}
