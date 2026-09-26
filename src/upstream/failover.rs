use crate::provider::ResolvedTarget;
use reqwest::StatusCode;

/// Determines whether an HTTP status code indicates a temporary failure eligible for failover.
pub fn is_failover_status(status: StatusCode) -> bool {
    matches!(
        status,
        StatusCode::TOO_MANY_REQUESTS        // 429
        | StatusCode::INTERNAL_SERVER_ERROR  // 500
        | StatusCode::BAD_GATEWAY           // 502
        | StatusCode::SERVICE_UNAVAILABLE   // 503
        | StatusCode::GATEWAY_TIMEOUT       // 504
    )
}

/// Result of an upstream forwarding execution, indicating if failover occurred.
pub struct ForwardResult {
    pub response: reqwest::Response,
    pub effective_target: ResolvedTarget,
    pub fallback_occurred: bool,
    pub original_provider: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_failover_status() {
        assert!(is_failover_status(StatusCode::TOO_MANY_REQUESTS));
        assert!(is_failover_status(StatusCode::INTERNAL_SERVER_ERROR));
        assert!(is_failover_status(StatusCode::BAD_GATEWAY));
        assert!(is_failover_status(StatusCode::SERVICE_UNAVAILABLE));
        assert!(is_failover_status(StatusCode::GATEWAY_TIMEOUT));

        assert!(!is_failover_status(StatusCode::OK));
        assert!(!is_failover_status(StatusCode::BAD_REQUEST));
        assert!(!is_failover_status(StatusCode::UNAUTHORIZED));
        assert!(!is_failover_status(StatusCode::NOT_FOUND));
    }
}
