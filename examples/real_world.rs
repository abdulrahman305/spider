//! `cargo run --example real_world --features="chrome chrome_intercept spider_utils/transformations"`

extern crate spider;
use crate::spider::tokio::io::AsyncWriteExt;
use spider::tokio;
use spider::website::Website;
use spider::{
    configuration::WaitForIdleNetwork, features::chrome_common::RequestInterceptConfiguration,
};
use spider_utils::spider_transformations::transformation::content::{
    transform_content, ReturnFormat, TransformConfig,
};
use std::io::Result;
use std::time::Duration;

async fn crawl_website(url: &str) -> Result<()> {
    let mut website: Website = Website::new(url)
        .with_limit(1)
        .with_chrome_intercept(RequestInterceptConfiguration::new(true))
        .with_wait_for_idle_network(Some(WaitForIdleNetwork::new(Some(Duration::from_millis(
            200,
        )))))
        .with_stealth(true)
        .with_return_page_links(true)
        .with_fingerprint(true)
        .with_proxies(Some(vec!["http://localhost:8888".into()]))
        .with_chrome_connection(Some("http://127.0.0.1:9222/json/version".into()))
        .build()
        .unwrap();

    let mut rx2 = website.subscribe(16).unwrap();
    let mut stdout = tokio::io::stdout();
    let mut conf = TransformConfig::default();
    conf.return_format = ReturnFormat::Markdown;

    tokio::spawn(async move {
        while let Ok(page) = rx2.recv().await {
            let _ = stdout
                .write_all(
                    format!(
                        "- {} -- Bytes transferred {:?} -- HTML Size {:?} -- Links: {:?}\n",
                        page.get_url(),
                        page.bytes_transferred.unwrap_or_default(),
                        page.get_html_bytes_u8().len(),
                        match page.page_links {
                            Some(ref l) => l.len(),
                            _ => 0,
                        }
                    )
                    .as_bytes(),
                )
                .await;

            let markup = transform_content(&page, &conf, &None, &None, &None);

            let _ = stdout
                .write_all(format!("- {}\n {}\n", page.get_url(), markup).as_bytes())
                .await;
        }
    });

    let start = crate::tokio::time::Instant::now();
    website.crawl().await;

    let duration = start.elapsed();

    let links = website.get_links();

    println!(
        "Time elapsed in website.crawl({}) is: {:?} for total pages: {:?}",
        website.get_url(),
        duration,
        links.len()
    );

    Ok(())
}

#[tokio::main]
async fn main() -> Result<()> {
    let _ = tokio::join!(
        crawl_website("https://choosealicense.com"),
        crawl_website("https://jeffmendez.com"),
        crawl_website("https://example.com"),
    );

    Ok(())
}