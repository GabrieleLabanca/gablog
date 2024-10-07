use chrono::{Datelike, Utc}; // Add Datelike trait
use pulldown_cmark::{html, Options, Parser}; // Added Event and Tag
use serde_json::json;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use tera::{Context, Tera}; // Add this line

const PATH_OUT_ROOT: &str = "public";
const PATH_OUT_PAGES: &str = "public/pages";
const CONTENT_PAGES_DIR: &str = "content/pages";

fn main() {
    let tera = match Tera::new("templates/*") {
        Ok(t) => t,
        Err(e) => {
            eprintln!("Failed to initialize Tera: {}", e);
            return;
        }
    };

    let file_paths = match list_files_in_pages() {
        Ok(paths) => paths,
        Err(e) => {
            eprintln!("Error listing files: {}", e);
            return;
        }
    };

    let mut file_info = Vec::new();
    for path in file_paths {
        let (title, path, ext, date, tags, category) = process_file(&tera, &path);
        if let Some(ext) = ext {
            if ext == "html" || ext == "md" {
                file_info.push((title, path, date, tags, category));
            }
        }
    }


    file_info.sort_by(|a, b| b.2.cmp(&a.2));

    // Generate the homepage with the list of articles
    if let Err(e) = generate_homepage(&tera, &file_info) {
        eprintln!("Error generating homepage: {}", e);
    }

    // Copy static files to site_artifacts
    if let Err(e) = copy_static_files() {
        eprintln!("Error copying static files: {}", e);
    }
}

fn process_file(
    tera: &Tera,
    path: &Path,
) -> (String, PathBuf, Option<String>, String, Vec<String>, String) {
    let extension = path.extension().and_then(|ext| ext.to_str());

    match extension {
        Some("md") | Some("html") => match read_file_to_string(path) {
            Ok(content) => {
                let parts: Vec<&str> = content.splitn(3, "---").collect();
                if parts.len() < 2 {
                    eprintln!(
                        "Error parsing YAML header in {}: Invalid YAML header format",
                        path.display()
                    );
                    return (
                        String::new(),
                        PathBuf::new(),
                        None,
                        String::new(),
                        Vec::new(),
                        String::new(),
                    );
                }
                let (header, body) = (parts[1], parts[2]);
                let yaml: serde_yaml::Value = match serde_yaml::from_str(header.trim()) {
                    Ok(yaml) => yaml,
                    Err(e) => {
                        eprintln!("{:?}: Failed to parse YAML header: {}", path, e);
                        eprintln!("String to parse: {}", header.trim());
                        return (
                            String::new(),
                            PathBuf::new(),
                            None,
                            String::new(),
                            Vec::new(),
                            String::new(),
                        );
                    }
                };
                let title = match yaml["title"].as_str() {
                    Some(t) => t.to_string(),
                    None => {
                        eprintln!("Error: Title not found in YAML header: {:?}", yaml);
                        return (
                            String::new(),
                            PathBuf::new(),
                            None,
                            String::new(),
                            Vec::new(),
                            String::new(),
                        );
                    }
                };
                let date = match yaml["date"].as_str() {
                    Some(d) => d.to_string(),
                    None => {
                        eprintln!("Error: Date not found in YAML header: {:?}", yaml);
                        return (
                            String::new(),
                            PathBuf::new(),
                            None,
                            String::new(),
                            Vec::new(),
                            String::new(),
                        );
                    }
                };
                let tags = match yaml["tags"].as_sequence() {
                    Some(tags) => tags
                        .iter()
                        .map(|tag| tag.as_str().unwrap_or("").to_string())
                        .collect(),
                    None => Vec::new(),
                };
                let category = match yaml["category"].as_str() {
                    Some(category) => category.to_string(),
                    None => String::new(),
                };
                let processed_body = if extension == Some("md") {
                    markdown_to_html(body.trim())
                } else {
                    body.trim().to_string()
                };

                let mut context = Context::new();
                context.insert("title", &title);
                context.insert("body", &processed_body);
                context.insert("date", &date);

                // Generate relative path for stylesheet and index
                let relative_path = path.parent().map_or("../".to_string(), |parent| {
                    let path_out_pages = Path::new(CONTENT_PAGES_DIR);
                    let path_diff = parent.strip_prefix(&path_out_pages).unwrap_or(parent);

                    // Create a relative path by counting the directories in path_diff
                    let mut relative_path = String::new();
                    for _ in path_diff.iter() {
                        relative_path.push_str("../");
                    }
                    relative_path
                });

                context.insert(
                    "styleSheetPath",
                    &format!("{}../css/article.css", relative_path),
                );
                context.insert("indexPath", &format!("{}../index.html", relative_path));

                match tera.render("article.html", &context) {
                    Ok(rendered) => {
                        let output_path = get_output_path(path);
                        if let Err(e) = write_to_page(&output_path, &rendered) {
                            eprintln!("Error writing to {}: {}", output_path.display(), e);
                        }
                        (
                            title,
                            output_path
                                .strip_prefix(PATH_OUT_ROOT)
                                .unwrap_or(&output_path)
                                .to_path_buf(),
                            Some(extension.unwrap().to_string()),
                            date,
                            tags,
                            category,
                        )
                    }
                    Err(e) => {
                        eprintln!("Error rendering template for {}: {}", path.display(), e);
                        (
                            String::new(),
                            PathBuf::new(),
                            None,
                            String::new(),
                            Vec::new(),
                            String::new(),
                        )
                    }
                }
            }
            Err(e) => {
                eprintln!("Error reading file {}: {}", path.display(), e);
                (
                    String::new(),
                    PathBuf::new(),
                    None,
                    String::new(),
                    Vec::new(),
                    String::new(),
                )
            }
        },
        Some("png") | Some("svg") | Some("jpg") => {
            let output_path = get_output_path(path);
            if let Some(parent) = output_path.parent() {
                fs::create_dir_all(parent).expect("Error creating output directory");
            }
            if let Err(e) = fs::copy(path, &output_path) {
                eprintln!("Error copying file to {}: {}", output_path.display(), e);
            }
            (
                output_path.file_name().unwrap().to_string_lossy().to_string(),
                output_path.strip_prefix(PATH_OUT_ROOT).unwrap_or(&output_path).to_path_buf(),
                Some(extension.unwrap().to_string()),
                String::new(), // Empty date
                Vec::new(),    // Empty tags
                String::new(), // Empty category
            )
        }
        _ => {
            println!("Skipping unsupported file type: {}", path.display());
            (
                String::new(),
                PathBuf::new(),
                None,
                String::new(),
                Vec::new(),
                String::new(),
            )
        }
    }
}

fn get_output_path(input_path: &Path) -> PathBuf {
    let mut output_path = PathBuf::from(PATH_OUT_PAGES);
    output_path.push(input_path.strip_prefix(CONTENT_PAGES_DIR).unwrap()); // Preserve directory structure
    if input_path.extension() == Some("md".as_ref()) {
        output_path.set_extension("html");
    }
    output_path
}

fn write_to_page(output_path: &Path, content: &str) -> io::Result<()> {
    if let Some(parent) = output_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(output_path, content)
}

fn read_file_to_string(file_path: &Path) -> io::Result<String> {
    fs::read_to_string(file_path)
}

fn markdown_to_html(markdown: &str) -> String {
    let mut options = Options::empty();
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TABLES);

    let parser = Parser::new_ext(markdown, options);
    let mut html_output = String::new();
    html::push_html(&mut html_output, parser);

    html_output
}

fn list_files_in_pages() -> io::Result<Vec<PathBuf>> {
    let mut file_paths = Vec::new();
    fn visit_dirs(dir: &Path, file_paths: &mut Vec<PathBuf>) -> io::Result<()> {
        if dir.is_dir() {
            for entry in fs::read_dir(dir)? {
                let entry = entry?;
                let path = entry.path();
                if path.is_dir() {
                    visit_dirs(&path, file_paths)?;
                } else {
                    file_paths.push(path);
                }
            }
        }
        Ok(())
    }

    visit_dirs(Path::new(CONTENT_PAGES_DIR), &mut file_paths)?;
    Ok(file_paths)
}

fn generate_homepage(
    tera: &Tera,
    articles: &Vec<(String, PathBuf, String, Vec<String>, String)>,
) -> Result<(), tera::Error> {
    let mut context = Context::new();
    context.insert("site_title", "gablog"); // Set your site title
    context.insert("current_year", &Utc::now().year().to_string());

    // Prepare articles for the context
    
    let articles_context: Vec<_> = articles
        .iter()
        .map(|(title, path, date, tags, category)| {
            let path_str = path
                .strip_prefix(PATH_OUT_ROOT)
                .unwrap_or(path)
                .to_string_lossy()
                .to_string();
            json!({
                "title": title,
                "path": path_str,
                "date": date,
                "tags": tags,
                "category": category
                // You can add a date here if you have it available
            })
        })
        .collect();

    context.insert("articles", &articles_context);

    // Render the homepage template
    let rendered = tera.render("home.html", &context)?;
    let output_path = PathBuf::from(PATH_OUT_ROOT).join("index.html");
    write_to_page(&output_path, &rendered)?;

    Ok(())
}

fn copy_static_files() -> io::Result<()> {
    let static_dir = Path::new("static");
    let output_dir = Path::new(PATH_OUT_ROOT);

    fn copy_dir_recursively(src: &Path, dest: &Path) -> io::Result<()> {
        if src.is_dir() {
            fs::create_dir_all(dest)?; // Create the destination directory
            for entry in fs::read_dir(src)? {
                let entry = entry?;
                let path = entry.path();
                let dest_path = dest.join(path.file_name().unwrap());
                if path.is_dir() {
                    copy_dir_recursively(&path, &dest_path)?; // Recur for subdirectories
                } else {
                    fs::copy(&path, &dest_path)?; // Copy files
                }
            }
        }
        Ok(())
    }

    copy_dir_recursively(static_dir, output_dir)?;
    Ok(())
}
