use anyhow::{anyhow, Context, Result};
use std::{env::args, str::from_utf8};
use tokio::{
    fs::File,
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
    spawn,
};

const MAX_REQUEST_SIZE: usize = 1024 * 32; // 32 KB

#[allow(dead_code)]
struct RequestLine {
    method: String,
    target: String,
    version: String,
}

#[allow(dead_code)]
struct Header {
    key: String,
    value: String,
}

#[allow(dead_code)]
struct Request {
    request_line: RequestLine,
    headers: Vec<Header>,
    body: String,
}

#[allow(dead_code)]
struct StatusLine {
    version: String,
    status_code: i32,
    status_text: String,
}

#[allow(dead_code)]
struct Response {
    status_line: StatusLine,
    headers: Vec<Header>,
    body: String,
}

enum Endpoint {
    Index,
    Echo(String),
    UserAgent,
    File(String),
    NotFound,
}

async fn handle_client(mut stream: TcpStream) -> Result<()> {
    let mut buf = [0; MAX_REQUEST_SIZE];
    let bytes_read = stream
        .read(&mut buf)
        .await
        .context("couldn't read from TCP stream")?;
    let request_string = from_utf8(&buf[..bytes_read])?;

    let request = parse_str_to_request(request_string).context("couldn't parse HTTP request")?;

    let endpoint = parse_target(request.request_line.target);
    let response = match endpoint {
        Endpoint::Index => Response {
            status_line: StatusLine {
                version: "HTTP/1.1".to_owned(),
                status_code: 200,
                status_text: "OK".to_owned(),
            },
            headers: Vec::new(),
            body: String::new(),
        },
        Endpoint::Echo(body) => respond("text/plain", body),
        Endpoint::UserAgent => {
            let user_agent_header = request
                .headers
                .iter()
                .find(|header| header.key == "User-Agent");
            let user_agent = match user_agent_header {
                None => "".to_owned(),
                Some(header) => header.value.to_owned(),
            };
            respond("text/plain", user_agent)
        }
        Endpoint::File(path) => {
            let args: Vec<String> = args().collect();
            let directory = match args.iter().position(|arg| arg == "--directory") {
                None => "/",
                Some(i) => match args.get(i + 1) {
                    None => "/",
                    Some(arg) => arg,
                },
            };
            let file_path = directory.to_owned() + path.as_str();
            let file_result = File::open(file_path).await;

            if let Ok(mut file) = file_result {
                let mut contents = String::new();
                file.read_to_string(&mut contents).await?;
                respond("application/octet-stream", contents)
            } else {
                not_found()
            }
        }
        Endpoint::NotFound => not_found(),
    };

    let response_str = parse_response_to_str(response);

    stream.write_all(response_str.as_bytes()).await?;
    println!("accepted new connection");
    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let listener = TcpListener::bind("127.0.0.1:4221").await?;

    loop {
        let (socket, _) = listener.accept().await?;

        spawn(async move { handle_client(socket).await });
    }
}

fn parse_target(target: String) -> Endpoint {
    let mut components = target.split("/");
    match components.next() {
        None => return Endpoint::NotFound,
        Some(string) => {
            if !string.is_empty() {
                return Endpoint::NotFound;
            }
        }
    }

    let route = match components.next() {
        None => return Endpoint::NotFound,
        Some(string) => string,
    };

    match route {
        "" => return Endpoint::Index,
        "echo" => match components.next() {
            None => return Endpoint::NotFound,
            Some(string) => return Endpoint::Echo(string.to_owned()),
        },
        "user-agent" => return Endpoint::UserAgent,
        "files" => match components.next() {
            None => return Endpoint::NotFound,
            Some(string) => return Endpoint::File(string.to_owned()),
        },
        _ => return Endpoint::NotFound,
    }
}

fn parse_str_to_request(request: &str) -> Result<Request> {
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
            let (key, value) = header.split_once(": ")?;
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

fn parse_response_to_str(response: Response) -> String {
    // deal with status line
    let status_line = response.status_line;
    let mut parsed_response = [
        status_line.version,
        status_line.status_code.to_string(),
        status_line.status_text,
    ]
    .join(" ");
    parsed_response += "\r\n";

    // deal with headers
    let parsed_headers: Vec<String> = response
        .headers
        .iter()
        .map(|header| String::new() + header.key.as_str() + ": " + header.value.as_str())
        .collect();
    parsed_response += parsed_headers.join("\r\n").as_str();
    parsed_response += "\r\n\r\n";

    // deal with body
    parsed_response += response.body.as_str();

    parsed_response
}

fn respond(content_type: &str, contents: String) -> Response {
    Response {
        status_line: StatusLine {
            version: "HTTP/1.1".to_owned(),
            status_code: 200,
            status_text: "OK".to_owned(),
        },
        headers: vec![
            Header {
                key: "Content-Type".to_owned(),
                value: content_type.to_owned(),
            },
            Header {
                key: "Content-Length".to_owned(),
                value: contents.len().to_string(),
            },
        ],
        body: contents,
    }
}

fn not_found() -> Response {
    Response {
        status_line: StatusLine {
            version: "HTTP/1.1".to_owned(),
            status_code: 404,
            status_text: "Not Found".to_owned(),
        },
        headers: Vec::new(),
        body: String::new(),
    }
}
