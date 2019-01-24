use std::env;
use std::fs::File;
use std::io;
use std::io::prelude::*;
use std::path::Path;
use std::process;

extern crate open;
extern crate pulldown_cmark;

fn main() {
    let args: Vec<String> = env::args().collect();
    if args.len() < 2 {
        println!("Must supply file name");
        process::exit(2);
    }
    let md_filename = &args[1];
    let html_filename = format!("{}.html", md_filename);
    render_to_html(md_filename, &html_filename).unwrap();
    open::that(html_filename).expect("Failed to open temp html file");
}

fn render_to_html<'a, P: AsRef<Path>>(md_filepath: P, html_filepath: P) -> Result<(), io::Error> {
    let mut f = File::open(md_filepath).expect("Failed to open file");
    let mut file_contents = String::new();
    f.read_to_string(&mut file_contents)?;
    let html_output = md_to_html(file_contents);
    let mut temp_html = File::create(html_filepath)?;
    temp_html.write_all(html_output.as_bytes())?;
    Ok(())
}

fn md_to_html(md_text: String) -> String {
    let parser = pulldown_cmark::Parser::new(md_text.as_str());
    let mut html_buf = String::new();
    pulldown_cmark::html::push_html(&mut html_buf, parser);
    md_html_wrapper(html_buf)
}

fn md_html_wrapper(content: String) -> String {
    format!(
        r##"<!doctype html>
<html>
    <head>
        <meta name="viewport" content="width=device-width, initial-scale=1">
        <link rel="stylesheet" href="https://sindresorhus.com/github-markdown-css/github-markdown.css">
        <style>
	.markdown-body {{
		box-sizing: border-box;
		min-width: 200px;
		max-width: 980px;
		margin: 0 auto;
		padding: 45px;
	}}

	@media (max-width: 767px) {{
		.markdown-body {{
			padding: 15px;
                }}
        }}
        </style>
    </head>
    <body>
        <div class="markdown-body">
            {}
        </div>
    </body>
</html>
            "##,
        content
    )
}
