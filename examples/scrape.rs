//! `cargo run --example scrape`
extern crate env_logger;
extern crate spider;

use env_logger::Env;
use spider::tokio;
use spider::website::Website;

#[tokio::main]
async fn main() {
    use std::io::{stdout, Write};

    let env = Env::default()
        .filter_or("RUST_LOG", "info")
        .write_style_or("RUST_LOG_STYLE", "always");

    env_logger::init_from_env(env);
    let target = "https://spider.cloud";
    let mut website: Website = Website::new(target);
    website.configuration.respect_robots_txt = true;
    website.configuration.delay = 15; // Defaults to 250 ms
    website.configuration.user_agent = Some(Box::new("SpiderBot".into())); // Defaults to spider/x.y.z, where x.y.z is the library version

    website.scrape().await;

    let mut lock = stdout().lock();

    let separator = "-".repeat(target.len());

    for page in website.get_pages().unwrap().iter() {
        writeln!(
            lock,
            "{}\n{}\n\n{}\n\n{}",
            separator,
            page.get_url(),
            page.get_html(),
            separator
        )
        .unwrap();
    }
}
