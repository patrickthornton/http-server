use anyhow::{anyhow, Context, Result};
use std::io::{Read, Write};
use std::net::TcpListener;

struct RequestLine {
    method: String,
    target: String,
    version: String,
}

struct Header {
    key: String,
    value: String,
}

struct Request {
    request_line: RequestLine,
    headers: Vec<Header>,
    body: String,
}

fn main() -> Result<()> {
    let listener = TcpListener::bind("127.0.0.1:4221")?;

    for stream in listener.incoming() {
        match stream {
            Ok(mut stream) => {
                let mut request_string = String::new();
                stream
                    .read_to_string(&mut request_string)
                    .context("couldn't read from TCP stream")?;

                dbg!(&request_string);

                let request =
                    parse_request(request_string).context("couldn't parse HTTP request")?;

                // check for blank path
                let response = match request.request_line.target.as_str() {
                    "/" => "HTTP/1.1 200 OK\r\n\r\n",
                    _ => "HTTP/1.1 404 Not Found\r\n\r\n",
                };

                stream.write_all(response.as_bytes())?;
                println!("accepted new connection");
            }
            Err(e) => {
                println!("error: {}", e);
            }
        }
    }
    Ok(())
}

// returns Error on a bad parse
fn parse_request(request: String) -> Result<Request> {
    // split into request line and headers at CRLF
    let (request_line, headers_and_body) = request
        .split_once("\r\n")
        .context("couldn't find first CRLF")?;

    // parse request line
    let request_line_components: Vec<&str> = request_line.split(" ").collect();
    if request_line_components.len() < 3 {
        return Err(anyhow!("not enough components in request line"));
    }
    let parsed_request_line = RequestLine {
        method: request_line_components[0].to_owned(),
        target: request_line_components[1].to_owned(),
        version: request_line_components[2].to_owned(),
    };

    // split off body
    let (headers, body) = headers_and_body
        .split_once("\r\n\r\n")
        .context("couldn't find double CRLF")?;

    // parse headers
    let parsed_headers: Vec<Header> = headers
        .split("\r\n")
        .map(|header| {
            let (key, value) = header.split_once(" ")?;
            Some(Header {
                key: key.to_owned(),
                value: value.to_owned(),
            })
        })
        .collect::<Option<Vec<Header>>>()
        .context("failed to parse headers")?;

    // return final product
    Ok(Request {
        request_line: parsed_request_line,
        headers: parsed_headers,
        body: body.to_owned(),
    })
}
