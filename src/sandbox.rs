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

    // Deterministic replacement order: sort keys so nested variable values
    // do not influence each other based on HashMap iteration order.
    let mut keys: Vec<&String> = variables.keys().collect();
    keys.sort();
    for key in keys {
        let value = &variables[key];
        result = result.replace(&format!("{{{{{key}}}}}"), value);
    }

    result
}

/// Parse a schemeless network target into `(host, optional port)`.
///
/// Correctly handles the three shapes the old naive `split(':')` mishandled:
/// - bracketed IPv6 with an optional port: `[::1]` / `[::1]:443`
/// - bare IPv6 (colons are part of the address, so there is NO port): `2001:db8::1`
/// - `host:port` for IPv4/hostnames: `example.com:8080`
///
/// The old code split on the first `:`, so `[::1]:443` yielded host `[` and every
/// schemeless target reported port 0.
fn split_schemeless_host_port(target: &str) -> (String, Option<u16>) {
    let target = target.split('/').next().unwrap_or(target);

    // Bracketed IPv6: `[addr]` or `[addr]:port`.
    if let Some(rest) = target.strip_prefix('[') {
        if let Some((addr, after)) = rest.split_once(']') {
            let port = after.strip_prefix(':').and_then(|p| p.parse().ok());
            return (addr.to_string(), port);
        }
        // No closing bracket: malformed, treat the remainder as the host.
        return (rest.to_string(), None);
    }

    // No brackets: distinguish `host:port` (one colon) from a bare IPv6 address
    // (two or more colons, which cannot carry a port without brackets).
    match target.matches(':').count() {
        1 => {
            let (host, port) = target.split_once(':').unwrap_or((target, ""));
            (host.to_string(), port.parse().ok())
        }
        n if n >= 2 => (target.to_string(), None),
        _ => (target.to_string(), None),
    }
}

/// Robustly extract hostname from a URL or raw network target.
#[must_use]
pub fn extract_hostname(target: &str) -> String {
    if let Ok(url) = url::Url::parse(target) {
        if let Some(host) = url.host_str() {
            return host.to_string();
        }
    }
    split_schemeless_host_port(target).0
}

/// Robustly extract port from a URL or network target string.
#[must_use]
pub fn extract_port(target: &str) -> String {
    if let Ok(url) = url::Url::parse(target) {
        if let Some(port) = url.port_or_known_default() {
            return port.to_string();
        }
    }
    split_schemeless_host_port(target)
        .1
        .map_or_else(|| "0".to_string(), |port| port.to_string())
}

#[cfg(test)]
mod tests {
    use super::{extract_hostname, extract_port, split_schemeless_host_port, substitute_target_vars};

    #[test]
    fn bracketed_ipv6_with_port() {
        assert_eq!(
            split_schemeless_host_port("[2001:db8::1]:8443"),
            ("2001:db8::1".to_string(), Some(8443))
        );
        assert_eq!(extract_hostname("[2001:db8::1]:8443"), "2001:db8::1");
        assert_eq!(extract_port("[2001:db8::1]:8443"), "8443");
    }

    #[test]
    fn bracketed_ipv6_without_port() {
        assert_eq!(extract_hostname("[::1]"), "::1");
        assert_eq!(extract_port("[::1]"), "0");
    }

    #[test]
    fn bare_ipv6_has_no_port() {
        // Colons belong to the address; the whole thing is the host.
        assert_eq!(extract_hostname("2001:db8::1"), "2001:db8::1");
        assert_eq!(extract_port("2001:db8::1"), "0");
    }

    #[test]
    fn ipv4_host_with_port() {
        assert_eq!(extract_hostname("192.168.1.1:9000"), "192.168.1.1");
        assert_eq!(extract_port("192.168.1.1:9000"), "9000");
    }

    #[test]
    fn hostname_without_port() {
        assert_eq!(extract_hostname("example.com"), "example.com");
        assert_eq!(extract_port("example.com"), "0");
    }

    #[test]
    fn strips_path_before_parsing_host() {
        assert_eq!(extract_hostname("example.com:8080/some/path"), "example.com");
        assert_eq!(extract_port("example.com:8080/some/path"), "8080");
    }

    #[test]
    fn full_url_still_parsed_by_url_crate() {
        assert_eq!(extract_hostname("https://example.com:8443/x"), "example.com");
        assert_eq!(extract_port("https://example.com:8443/x"), "8443");
        // Known-default port when omitted.
        assert_eq!(extract_port("https://example.com"), "443");
    }

    #[test]
    fn substitution_is_deterministic() {
        use std::collections::HashMap;
        let mut variables = HashMap::new();
        variables.insert("A".to_string(), "B".to_string());
        variables.insert("B".to_string(), "C".to_string());

        // If A is substituted first, result is "B"; if B is substituted first,
        // the A value "B" is also replaced to "C", yielding "C". Sorting by
        // key means A is always substituted first, so the result is stable.
        assert_eq!(
            substitute_target_vars("{{A}}", "http://example.com", &variables),
            "B"
        );
    }
}
