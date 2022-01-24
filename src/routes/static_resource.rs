use crate::route_prelude::*;

/// Serve a file from `/templates/static/`
pub(crate) async fn static_resource(req: Request<()>) -> tide::Result {
  use http::conditional::{IfModifiedSince, LastModified};

  let file_mime_option = StaticDir::get(req.param("path")?).map(|file| {
    (
      file,
      http::Mime::from_extension(
        Path::new(&req.param("path").unwrap())
          .extension()
          .unwrap()
          .to_string_lossy(),
      )
      .unwrap_or(http::mime::PLAIN),
    )
  });

  match file_mime_option {
    Some((file, mime)) => {
      let metadata = file.metadata;
      let last_modified = std::time::SystemTime::UNIX_EPOCH
        + std::time::Duration::from_millis(metadata.last_modified().unwrap());

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
        response.insert_header("Content-Length", file.data.len().to_string());

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
          response.insert_header("Content-Length", file.data.len().to_string());
        }
        http::Method::Get => {
          // load the file from disk
          response.set_body(&*file.data);
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
