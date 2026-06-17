use anyhow::{Context, Result};

use crate::auth;
use crate::config::Config;

/// Artifact name prefix used by the benchmark workflow.
const ARTIFACT_PREFIX: &str = "benchmark-report_";

/// Attempts to download all benchmark report artifacts from a workflow run
/// and post a combined comment to the associated PR.
pub async fn try_post_benchmark_comment(
    config: &Config,
    owner: &str,
    repo: &str,
    run_id: u64,
    head_sha: &str,
) -> Result<()> {
    let octocrab = auth::octocrab_for_installation(
        config.app_id,
        &config.private_key,
        config.installation_id,
    )
    .await?;

    // Find the PR associated with this head SHA
    let pr_number = find_pr_for_sha(&octocrab, owner, repo, head_sha)
        .await?
        .ok_or_else(|| anyhow::anyhow!("No open PR found for SHA {head_sha}"))?;

    tracing::info!("Found PR #{pr_number} for SHA {head_sha}");

    // List artifacts for this workflow run
    let artifacts = list_benchmark_artifacts(&octocrab, owner, repo, run_id).await?;

    if artifacts.is_empty() {
        tracing::info!("No benchmark artifacts found for run {run_id}");
        return Ok(());
    }

    // Download and combine all benchmark report artifacts
    let mut sections = Vec::new();
    for artifact in &artifacts {
        let label = artifact
            .name
            .strip_prefix(ARTIFACT_PREFIX)
            .unwrap_or(&artifact.name);

        match download_artifact_text(&octocrab, owner, repo, artifact.id).await {
            Ok(content) => {
                sections.push(format!(
                    "<details>\n<summary>{label}</summary>\n\n{content}\n</details>"
                ));
            }
            Err(e) => {
                tracing::warn!("Failed to download artifact '{}': {e:#}", artifact.name);
            }
        }
    }

    if sections.is_empty() {
        return Ok(());
    }

    let body = format!(
        "## Benchmark Results\n\n{}\n\n<sub>Posted by hyperlight-gh-bot for commit {head_sha}</sub>",
        sections.join("\n\n"),
    );

    // Upsert the comment (update existing bot comment or create new one)
    upsert_pr_comment(&octocrab, owner, repo, pr_number, &body).await?;

    tracing::info!("Posted benchmark comment on PR #{pr_number}");
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

/// Minimal artifact info.
struct ArtifactInfo {
    id: u64,
    name: String,
}

/// Lists benchmark report artifacts for a workflow run.
async fn list_benchmark_artifacts(
    octocrab: &octocrab::Octocrab,
    owner: &str,
    repo: &str,
    run_id: u64,
) -> Result<Vec<ArtifactInfo>> {
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

    Ok(artifacts
        .into_iter()
        .filter_map(|a| {
            let name = a["name"].as_str()?.to_string();
            if name.starts_with(ARTIFACT_PREFIX) {
                Some(ArtifactInfo {
                    id: a["id"].as_u64()?,
                    name,
                })
            } else {
                None
            }
        })
        .collect())
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
