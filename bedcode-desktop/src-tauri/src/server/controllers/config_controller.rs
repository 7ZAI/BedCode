//! Config Controller
//!
//! Routes:
//! - GET /api/configs
//! - GET /api/quick-actions

use actix_web::HttpResponse;
use crate::system::app_context::AppContext;
use crate::server::dtos::ApiResponse;
use crate::server::dtos::config_dto::*;
use crate::session::SessionConfigManager;

/// GET /api/configs
pub async fn list_configs() -> HttpResponse {
    let ctx = AppContext::global();
    let manager = SessionConfigManager::new(ctx.db().clone());

    match manager.list_configs().await {
        Ok(configs) => {
            let items: Vec<ConfigItem> = configs.into_iter().map(|c| ConfigItem {
                id: c.id,
                name: c.name,
                environment: c.environment,
                wsl_distro: c.wsl_distro,
                working_dir: c.working_dir,
                command: c.command,
            }).collect();
            let data = ConfigListResponseData { configs: items };
            HttpResponse::Ok().json(ApiResponse::ok_with_data(data))
        }
        Err(e) => {
            tracing::error!(error = %e, "Failed to list configs");
            HttpResponse::Ok().json(ApiResponse::<()>::error(500, &e.to_string()))
        }
    }
}

/// GET /api/quick-actions
pub async fn list_quick_actions() -> HttpResponse {
    let ctx = AppContext::global();
    let db = ctx.db();
    let db_guard = db.lock().await;

    match db_guard.get_quick_actions() {
        Ok(actions) => {
            let items: Vec<QuickActionItem> = actions.into_iter().map(|a| QuickActionItem {
                id: a.id,
                name: a.name,
                content: a.content,
                icon: a.icon,
                color: a.color,
            }).collect();
            let data = QuickActionListResponseData { actions: items };
            HttpResponse::Ok().json(ApiResponse::ok_with_data(data))
        }
        Err(e) => {
            tracing::error!(error = %e, "Failed to list quick actions");
            HttpResponse::Ok().json(ApiResponse::<()>::error(500, &e.to_string()))
        }
    }
}
