use warp::cors::Cors;

pub fn cors() -> Cors {
    warp::cors()
        .allow_origins(vec!["https://tunshell.com", "http://localhost:3003"])
        .allow_headers(vec![
            "Host",
            "User-Agent",
            "Accept",
            "Content-Type",
            "Sec-Fetch-Mode",
            "Referer",
            "Origin",
            "Authority",
            "Access-Control-Request-Method",
            "Access-Control-Request-Headers",
        ])
        .allow_methods(vec!["GET", "POST", "PUT", "PATCH", "DELETE"])
        .build()
}
