//! Code protocol sandbox execution helpers.

use std::collections::HashMap;
use std::time::Duration;
use tokio::process::Command;

/// # Errors
/// Returns an error if execution fails or times out.
pub async fn execute_script(
    engine: &str,
    code: &str,
    target: &str,
    template_id: &str,
    variables: &HashMap<String, String, impl std::hash::BuildHasher>,
    timeout_dur: Duration,
) -> std::io::Result<String> {
    let allowed_engines = [
        "bash",
        "sh",
        "python",
        "python3",
        "ruby",
        "node",
        "perl",
        "powershell",
    ];
    if !allowed_engines.contains(&engine) {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            format!("engine '{engine}' is not in the allowlist"),
        ));
    }

    let substituted = substitute_target_vars(code, target, variables);

    let (interpreter, run_args) = match engine {
        "python" | "python3" => ("python3", vec!["-c".to_string(), substituted]),
        "node" => ("node", vec!["-e".to_string(), substituted]),
        "ruby" => ("ruby", vec!["-e".to_string(), substituted]),
        "perl" => ("perl", vec!["-e".to_string(), substituted]),
        "powershell" => ("pwsh", vec!["-Command".to_string(), substituted]),
        _ => (engine, vec!["-c".to_string(), substituted]),
    };

    tracing::info!(
        engine = engine,
        target = target,
        "executing code protocol script in memory"
    );

    let hostname = extract_hostname(target);
    let port = extract_port(target);

    let result = tokio::time::timeout(
        timeout_dur,
        Command::new(interpreter)
            .args(&run_args)
            .env("TARGET", target)
            .env("HOSTNAME", &hostname)
            .env("BASE_URL", target)
            .env("PORT", &port)
            .env("TEMPLATE_ID", template_id)
            .output(),
    )
    .await
    .map_err(|_| std::io::Error::new(std::io::ErrorKind::TimedOut, "script timed out"))?;

    let output = result?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    if !stderr.is_empty() {
        tracing::debug!(template_id = %template_id, stderr = %stderr, "script stderr");
    }

    Ok(format!("{stdout}\n{stderr}"))
}

/// Substitutes template variables and target metadata into script source code.
#[must_use]
pub fn substitute_target_vars(
    code: &str,
    target: &str,
    variables: &HashMap<String, String, impl std::hash::BuildHasher>,
) -> String {
    let hostname = extract_hostname(target);
    let mut result = code
        .replace("{{BaseURL}}", target)
        .replace("{{Hostname}}", &hostname)
        .replace("{{Target}}", target)
        .replace("{{Host}}", &hostname);

    for (key, value) in variables {
        result = result.replace(&format!("{{{{{key}}}}}"), value);
    }

    result
}

/// Robustly extract hostname from a URL or raw network target.
#[must_use]
pub fn extract_hostname(target: &str) -> String {
    if let Ok(url) = url::Url::parse(target) {
        return url.host_str().unwrap_or_default().to_string();
    }
    target
        .split('/')
        .next()
        .unwrap_or(target)
        .split(':')
        .next()
        .unwrap_or(target)
        .to_string()
}

/// Robustly extract port from a URL or network target string.
#[must_use]
pub fn extract_port(target: &str) -> String {
    if let Ok(url) = url::Url::parse(target) {
        return url.port_or_known_default().unwrap_or(0).to_string();
    }
    "0".to_string()
}
