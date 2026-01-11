//! Docker image management commands for the sandbox.

use anyhow::{Context, Result};
use bollard::image::{BuildImageOptions, CreateImageOptions, ListImagesOptions};
use bollard::service::ImageSummary;
use bollard::Docker;
use bytes::Bytes;
use clap::Subcommand;
use futures_util::StreamExt;
use std::path::Path;
use tar::Builder;
use tracing::{info, warn};

use crate::config::Config;

/// Image management actions.
#[derive(Subcommand, Debug)]
pub enum ImageAction {
    /// Build the sandbox image from Dockerfile
    Build {
        /// Dockerfile path (default: ./Dockerfile)
        #[arg(long, default_value = "Dockerfile")]
        dockerfile: String,

        /// Image tag (default: from ralph.toml or "ralph:latest")
        #[arg(long)]
        tag: Option<String>,
    },

    /// Pull pre-built image from registry
    Pull {
        /// Image name to pull (default: from ralph.toml or "ralph:latest")
        #[arg(long)]
        image: Option<String>,
    },

    /// Show image status and information
    Status {
        /// Image name to check (default: from ralph.toml or "ralph:latest")
        #[arg(long)]
        image: Option<String>,
    },
}

/// Run image management command.
pub async fn run(action: ImageAction) -> Result<()> {
    // Load config to get default image name
    let project_dir = std::env::current_dir().context("Failed to get current directory")?;
    let config = Config::load(&project_dir)?;

    match action {
        ImageAction::Build { dockerfile, tag } => {
            let image_tag = tag.unwrap_or_else(|| config.sandbox.image.clone());
            build_image(&dockerfile, &image_tag, &project_dir).await?;
        }
        ImageAction::Pull { image } => {
            let image_name = image.unwrap_or_else(|| config.sandbox.image.clone());
            pull_image(&image_name).await?;
        }
        ImageAction::Status { image } => {
            let image_name = image.unwrap_or_else(|| config.sandbox.image.clone());
            show_image_status(&image_name).await?;
        }
    }

    Ok(())
}

/// Build Docker image from Dockerfile.
async fn build_image(dockerfile: &str, tag: &str, project_dir: &Path) -> Result<()> {
    info!("Building Docker image: {}", tag);

    let docker = Docker::connect_with_local_defaults()
        .context("Failed to connect to Docker. Is Docker running?")?;

    docker
        .ping()
        .await
        .context("Cannot ping Docker daemon. Is Docker running?")?;

    let dockerfile_path = project_dir.join(dockerfile);
    if !dockerfile_path.exists() {
        anyhow::bail!("Dockerfile not found: {}", dockerfile_path.display());
    }

    let build_options = BuildImageOptions {
        dockerfile: dockerfile.to_string(),
        t: tag.to_string(),
        ..Default::default()
    };

    // Create tarball from project directory
    let mut tar_buf = Vec::new();
    {
        let mut tar = Builder::new(&mut tar_buf);
        tar.append_dir_all(".", project_dir)
            .context("Failed to create tarball from project directory")?;
        tar.finish().context("Failed to finalize tarball")?;
    }
    let tar_bytes = Bytes::from(tar_buf);

    // Build image from tarball
    let mut stream = docker.build_image(build_options, None, Some(tar_bytes));

    info!("Building image from {}...", dockerfile);
    let mut last_output = String::new();

    loop {
        let chunk_result = stream.next().await;
        match chunk_result {
            Some(Ok(output)) => {
                // BuildInfo is a struct with fields, not a JSON value
                if let Some(stream_text) = &output.stream {
                    let trimmed = stream_text.trim();
                    if !trimmed.is_empty() {
                        print!("{trimmed}");
                        last_output = trimmed.to_string();
                    }
                } else if let Some(error) = &output.error {
                    anyhow::bail!("Docker build error: {error}");
                } else if let Some(error_detail) = &output.error_detail {
                    if let Some(message) = &error_detail.message {
                        anyhow::bail!("Docker build error: {message}");
                    }
                }
            }
            Some(Err(e)) => {
                anyhow::bail!("Error building image: {e}");
            }
            None => break,
        }
    }

    if last_output.contains("Successfully tagged") || last_output.contains("Successfully built") {
        info!("Image built successfully: {}", tag);
    } else {
        warn!("Build completed, but success message not found. Image may not be tagged correctly.");
    }

    Ok(())
}

/// Pull Docker image from registry.
async fn pull_image(image: &str) -> Result<()> {
    info!("Pulling Docker image: {}", image);

    let docker = Docker::connect_with_local_defaults()
        .context("Failed to connect to Docker. Is Docker running?")?;

    docker
        .ping()
        .await
        .context("Cannot ping Docker daemon. Is Docker running?")?;

    let pull_options = CreateImageOptions {
        from_image: image,
        ..Default::default()
    };

    let mut stream = docker.create_image(Some(pull_options), None, None);

    info!("Pulling image {}...", image);
    let mut last_output = String::new();

    loop {
        let chunk_result = stream.next().await;
        match chunk_result {
            Some(Ok(output)) => {
                // CreateImageInfo is a struct with fields, not a JSON value
                if let Some(status) = &output.status {
                    let trimmed = status.trim();
                    if !trimmed.is_empty() {
                        println!("{trimmed}");
                        last_output = trimmed.to_string();
                    }
                } else if let Some(error) = &output.error {
                    anyhow::bail!("Docker pull error: {error}");
                }
            }
            Some(Err(e)) => {
                anyhow::bail!("Error pulling image: {e}");
            }
            None => break,
        }
    }

    if last_output.contains("Downloaded") || last_output.contains("Already exists") {
        info!("Image pulled successfully: {}", image);
    } else {
        warn!("Pull completed, but success message not found.");
    }

    Ok(())
}

/// Show Docker image status and information.
async fn show_image_status(image: &str) -> Result<()> {
    let docker = Docker::connect_with_local_defaults()
        .context("Failed to connect to Docker. Is Docker running?")?;

    docker
        .ping()
        .await
        .context("Cannot ping Docker daemon. Is Docker running?")?;

    // List all images and find matching ones
    let images = docker
        .list_images(Some(ListImagesOptions::<String> {
            all: true,
            ..Default::default()
        }))
        .await
        .context("Failed to list images")?;

    // Parse image name and tag
    let (name, tag) = parse_image_tag(image);

    let matching_images: Vec<&ImageSummary> = images
        .iter()
        .filter(|img| {
            img.repo_tags.iter().any(|tag_str| {
                if let Some(colon_pos) = tag_str.rfind(':') {
                    let (n, t) = tag_str.split_at(colon_pos);
                    n == name && &t[1..] == tag
                } else {
                    tag_str == name && tag == "latest"
                }
            })
        })
        .collect();

    if matching_images.is_empty() {
        println!("Image not found: {image}");
        println!("\nTo build the image, run:");
        println!("  ralph image build");
        println!("\nTo pull the image, run:");
        println!("  ralph image pull");
        return Ok(());
    }

    println!("Image: {image}");
    println!("Status: Found");

    for img in matching_images {
        // Image size is i64, converting to f64 for display is safe for reasonable image sizes
        // Use absolute value to handle any negative values (shouldn't occur in practice)
        // Precision loss is acceptable for display purposes (image sizes won't exceed f64 precision)
        #[allow(clippy::cast_precision_loss)]
        let size = img.size.unsigned_abs() as f64;
        let size_megabytes = size / 1_048_576.0;
        let size_gigabytes = size_megabytes / 1024.0;
        if size_gigabytes >= 1.0 {
            println!("Size: {size_gigabytes:.2} GB ({size_megabytes:.2} MB)");
        } else {
            println!("Size: {size_megabytes:.2} MB");
        }

        println!("Created: {}", img.created);

        if !img.repo_tags.is_empty() {
            println!("Tags: {}", img.repo_tags.join(", "));
        }

        println!("ID: {}", img.id);
    }

    Ok(())
}

/// Parse image name and tag from a string.
fn parse_image_tag(image: &str) -> (&str, &str) {
    if let Some(colon_pos) = image.rfind(':') {
        let (name, tag) = image.split_at(colon_pos);
        (name, &tag[1..])
    } else {
        (image, "latest")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_image_tag() {
        let (name, tag) = parse_image_tag("ralph:latest");
        assert_eq!(name, "ralph");
        assert_eq!(tag, "latest");

        let (name2, tag2) = parse_image_tag("myregistry/ralph:v1.0");
        assert_eq!(name2, "myregistry/ralph");
        assert_eq!(tag2, "v1.0");

        let (name3, tag3) = parse_image_tag("registry.example.com:5000/ralph:dev");
        assert_eq!(name3, "registry.example.com:5000/ralph");
        assert_eq!(tag3, "dev");
    }

    #[test]
    fn test_parse_image_no_tag() {
        let (name, tag) = parse_image_tag("ralph");
        assert_eq!(name, "ralph");
        assert_eq!(tag, "latest");
    }

    #[test]
    fn test_parse_image_with_port() {
        // Test that we correctly handle images with ports in the registry name
        let (name, tag) = parse_image_tag("registry:5000/image:tag");
        assert_eq!(name, "registry:5000/image");
        assert_eq!(tag, "tag");
    }

    #[tokio::test]
    async fn test_show_image_status_no_docker() {
        // This test verifies the function handles Docker unavailability gracefully
        // It will skip if Docker is not available
        let result = show_image_status("nonexistent:image").await;

        // Function should either succeed (returning status) or fail with Docker connection error
        match result {
            Ok(()) => {
                // Successfully checked status (image not found or found)
                // This is valid
            }
            Err(e) => {
                // Docker not available - this is acceptable in test environments
                let error_msg = e.to_string();
                assert!(
                    error_msg.contains("Docker") || error_msg.contains("docker"),
                    "Unexpected error: {error_msg}"
                );
            }
        }
    }
}
