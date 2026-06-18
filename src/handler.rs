use anyhow::{Context, Result};

use crate::auth;
use crate::config::Config;
use crate::repo_config;

/// Downloads the comment artifact from a workflow run and posts it to the associated PR.
/// The artifact is expected to be a zip containing a single text file with the comment body.
pub async fn try_post_benchmark_comment(
    config: &Config,
    owner: &str,
    repo: &str,
    run_id: u64,
    head_sha: &str,
    installation_id: u64,
    job_name: &str,
) -> Result<()> {
    let octocrab = auth::octocrab_for_installation(
        config.app_id,
        &config.private_key,
        installation_id,
    )
    .await?;

    // Load per-repo config (or defaults if file not found)
    let repo_config = repo_config::load(&octocrab, owner, repo).await?;

    // Check if this job matches the configured filter
    let job_regex = repo_config.job_filter_regex()?;
    if !job_regex.is_match(job_name) {
        tracing::debug!(
            "Job '{job_name}' does not match filter '{}', skipping",
            repo_config.job_filter
        );
        return Ok(());
    }

    tracing::info!("Processing job '{job_name}' for SHA {head_sha}");

    // Find the PR associated with this head SHA
    let pr_number = find_pr_for_sha(&octocrab, owner, repo, head_sha)
        .await?
        .ok_or_else(|| anyhow::anyhow!("No open PR found for SHA {head_sha}"))?;

    tracing::info!("Found PR #{pr_number} for SHA {head_sha}");

    // Find the comment artifact
    let artifact_id = find_artifact(&octocrab, owner, repo, run_id, &repo_config.artifact_name)
        .await?
        .ok_or_else(|| {
            anyhow::anyhow!(
                "Artifact '{}' not found for run {run_id}",
                repo_config.artifact_name
            )
        })?;

    // Download the artifact content — this is the comment body
    let body = download_artifact_text(&octocrab, owner, repo, artifact_id).await?;

    // Upsert the comment (update existing bot comment or create new one)
    upsert_pr_comment(&octocrab, owner, repo, pr_number, &body).await?;

    tracing::info!("Posted comment on PR #{pr_number}");
    Ok(())
}

/// Finds an open PR whose head SHA matches the given SHA.
async fn find_pr_for_sha(
    octocrab: &octocrab::Octocrab,
    owner: &str,
    repo: &str,
    head_sha: &str,
) -> Result<Option<u64>> {
    let pulls = octocrab.pulls(owner, repo);
    let prs = pulls
        .list()
        .state(octocrab::params::State::Open)
        .send()
        .await
        .context("Failed to list pull requests")?;

    for pr in prs.items {
        if pr.head.sha == head_sha {
            return Ok(Some(pr.number));
        }
    }
    Ok(None)
}

/// Finds an artifact by exact name in a workflow run.
async fn find_artifact(
    octocrab: &octocrab::Octocrab,
    owner: &str,
    repo: &str,
    run_id: u64,
    artifact_name: &str,
) -> Result<Option<u64>> {
    let response: serde_json::Value = octocrab
        .get(
            format!("/repos/{owner}/{repo}/actions/runs/{run_id}/artifacts"),
            None::<&()>,
        )
        .await
        .context("Failed to list artifacts")?;

    let artifacts = response["artifacts"]
        .as_array()
        .cloned()
        .unwrap_or_default();

    Ok(artifacts.iter().find_map(|a| {
        let name = a["name"].as_str()?;
        if name == artifact_name {
            a["id"].as_u64()
        } else {
            None
        }
    }))
}

/// Downloads an artifact (expects a zip containing a single text file)
/// and returns its content as a string.
async fn download_artifact_text(
    octocrab: &octocrab::Octocrab,
    owner: &str,
    repo: &str,
    artifact_id: u64,
) -> Result<String> {
    // Get the download URL (GitHub returns a 302 redirect)
    let bytes: Vec<u8> = octocrab
        .get(
            format!("/repos/{owner}/{repo}/actions/artifacts/{artifact_id}/zip"),
            None::<&()>,
        )
        .await
        .context("Failed to download artifact")?;

    // The response is a zip file — extract the first file
    let cursor = std::io::Cursor::new(bytes);
    let mut archive = zip::ZipArchive::new(cursor).context("Failed to read artifact zip")?;

    let mut file = archive.by_index(0).context("Artifact zip is empty")?;
    let mut content = String::new();
    std::io::Read::read_to_string(&mut file, &mut content)?;

    Ok(content)
}

/// Updates an existing bot comment or creates a new one on the PR.
async fn upsert_pr_comment(
    octocrab: &octocrab::Octocrab,
    owner: &str,
    repo: &str,
    pr_number: u64,
    body: &str,
) -> Result<()> {
    let issues = octocrab.issues(owner, repo);

    // Look for an existing comment from our bot
    let comments = issues
        .list_comments(pr_number)
        .send()
        .await
        .context("Failed to list PR comments")?;

    let marker = "Posted by hyperlight-gh-bot";
    let existing = comments.items.iter().find(|c| {
        c.body.as_deref().is_some_and(|b| b.contains(marker))
    });

    if let Some(comment) = existing {
        issues
            .update_comment(comment.id, body)
            .await
            .context("Failed to update comment")?;
    } else {
        issues
            .create_comment(pr_number, body)
            .await
            .context("Failed to create comment")?;
    }

    Ok(())
}
