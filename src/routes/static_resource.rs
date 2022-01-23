use crate::route_prelude::*;

/// Serve a file from `/templates/static/`
pub(crate) async fn static_resource(req: Request<()>) -> tide::Result {
  use http::conditional::{IfModifiedSince, LastModified};

  let file_mime_option = STATIC_DIR.get_file(&req.url().path()[1..]).map(|file| {
    (
      file,
      http::Mime::from_extension(file.path().extension().unwrap().to_string_lossy())
        .unwrap_or(http::mime::PLAIN),
    )
  });

  // only use a File handle here because we might not need to load the file
  // let file_mime_option = match req.url().path() {
  //   "/style.css" => Some((
  //     File::open("templates/static/style.css").unwrap(),
  //     http::mime::CSS
  //   )),
  //   "/robots.txt" => Some((
  //     File::open("templates/static/robots.txt").unwrap(),
  //     http::mime::PLAIN
  //   )),
  //   "/Feed-icon.svg" => Some((
  //     File::open("templates/static/Feed-icon.svg").unwrap(),
  //     http::mime::SVG
  //   )),
  //   _ => None
  // };

  match file_mime_option {
    Some((file, mime)) => {
      let metadata = file.metadata().unwrap();
      let last_modified = metadata.modified();

      let header = IfModifiedSince::from_headers(&req)?;

      // check cache validating headers
      if matches!(header, Some(date) if IfModifiedSince::new(last_modified) <= date) {
        // the file has not changed
        let mut response = Response::new(304);
        response.set_content_type(mime);
        LastModified::new(last_modified).apply(&mut response);

        // A server MAY send a Content-Length header field in a 304
        // response to a conditional GET request; a server MUST NOT send
        // Content-Length in such a response unless its field-value equals
        // the decimal number of octets that would have been sent in the
        // payload body of a 200 response to the same request.
        // - RFC 7230 § 3.3.2
        response.insert_header("Content-Length", file.contents().len().to_string());

        return Ok(response);
      }

      let mut response = Response::new(200);

      match req.method() {
        http::Method::Head => {
          // A server MAY send a Content-Length header field in a
          // response to a HEAD request; a server MUST NOT send
          // Content-Length in such a response unless its field-value
          // equals the decimal number of octets that would have been
          // sent in the payload body of a response if the same request
          // had used the GET method.
          // - RFC 7230 § 3.3.2
          response.insert_header("Content-Length", file.contents().len().to_string());
        }
        http::Method::Get => {
          // load the file from disk
          response.set_body(file.contents());
        }
        _ => return Err(tide::Error::from_str(405, "")),
      }

      response.set_content_type(mime);
      LastModified::new(last_modified).apply(&mut response);
      Ok(response)
    }
    None if req.method() == http::Method::Get => {
      Err(tide::Error::from_str(404, "This page does not exist."))
    }
    // issue a 405 error since this is used as the catchall
    None => Err(tide::Error::from_str(405, "")),
  }
}
