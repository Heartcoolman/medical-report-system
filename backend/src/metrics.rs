use axum::extract::State;
use axum::response::IntoResponse;
use axum_prometheus::{metrics_exporter_prometheus::PrometheusHandle, PrometheusMetricLayer};
use crate::auth::{AuthUser, Role};
use crate::error::AppError;

pub fn setup_metrics() -> (PrometheusMetricLayer<'static>, PrometheusHandle) {
    PrometheusMetricLayer::pair()
}

pub async fn metrics_handler(
    auth: AuthUser,
    State(state): State<crate::AppState>,
) -> Result<impl IntoResponse, AppError> {
    let role = Role::from_str(&auth.0.role).unwrap_or(Role::ReadOnly);
    if !role.has_at_least(Role::Admin) {
        return Err(AppError::insufficient_role(Role::Admin.as_str()));
    }

    Ok(state.metrics_handle.render())
}
