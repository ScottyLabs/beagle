use axum::body::Bytes;
use axum::http::Response;
use slack_morphism::prelude::*;
use http_body_util::combinators::BoxBody;
use http_body_util::{BodyExt, Empty, Full};
use dotenv_codegen::dotenv;
use axum::Extension;
use std::convert::Infallible;
use log::info;
use std::sync::Arc;
use tokio::net::TcpListener;

const SLACK_CLIENT_ID: &str = dotenv!("SLACK_CLIENT_ID");
const SLACK_CLIENT_SECRET: &str = dotenv!("SLACK_CLIENT_SECRET");
const SLACK_BOT_SCOPE: &str = dotenv!("SLACK_BOT_SCOPE");
const SLACK_REDIRECT_HOST: &str = dotenv!("SLACK_REDIRECT_HOST");
const SLACK_SIGNING_SECRET: &str = dotenv!("SLACK_SIGNING_SECRET");

async fn oauth_install_function(
    resp: SlackOAuthV2AccessTokenResponse,
    _client: Arc<SlackHyperClient>,
    _states: SlackClientEventsUserState,
) {
    println!("{:#?}", resp);
}

async fn welcome_installed() -> String {
    "Welcome".to_string()
}

async fn cancelled_install() -> String {
    "Cancelled".to_string()
}

async fn error_install() -> String {
    "Error while installing".to_string()
}

async fn push_event(
    Extension(_environment): Extension<Arc<SlackHyperListenerEnvironment>>,
    Extension(event): Extension<SlackPushEvent>,
) -> Response<BoxBody<Bytes, Infallible>> {
    println!("Received push event: {:?}", event);

    match event {
        SlackPushEvent::UrlVerification(url_ver) => {
            Response::new(Full::new(url_ver.challenge.into()).boxed())
        }
        _ => Response::new(Empty::new().boxed()),
    }
}

async fn command_event(
    Extension(_environment): Extension<Arc<SlackHyperListenerEnvironment>>,
    Extension(event): Extension<SlackCommandEvent>,
) -> axum::Json<SlackCommandEventResponse> {
    println!("Received command event: {:?}", event);
    axum::Json(SlackCommandEventResponse::new(
        SlackMessageContent::new().with_text("Working on it".into()),
    ))
}

async fn interaction_event(
    Extension(_environment): Extension<Arc<SlackHyperListenerEnvironment>>,
    Extension(event): Extension<SlackInteractionEvent>,
) {
    println!("Received interaction event: {:?}", event);
}

fn error_handler(
    err: Box<dyn std::error::Error + Send + Sync>,
    _client: Arc<SlackHyperClient>,
    _states: SlackClientEventsUserState,
) -> HttpStatusCode {
    println!("{:#?}", err);

    // Defines what we return Slack server
    HttpStatusCode::BAD_REQUEST
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    env_logger::init();

    let client: Arc<SlackHyperClient> =
        Arc::new(SlackClient::new(SlackClientHyperConnector::new()?));

    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], 8080));
    info!("Loading server: {}", addr);

    let oauth_listener_config = SlackOAuthListenerConfig::new(
        SLACK_CLIENT_ID.into(),
        SLACK_CLIENT_SECRET.into(),
        SLACK_BOT_SCOPE.into(),
        SLACK_REDIRECT_HOST.into(),
    );

    let listener_environment: Arc<SlackHyperListenerEnvironment> = Arc::new(
        SlackClientEventsListenerEnvironment::new(client.clone())
            .with_error_handler(error_handler),
    );
    let signing_secret: SlackSigningSecret = SLACK_SIGNING_SECRET.into();

    let listener: SlackEventsAxumListener<SlackHyperHttpsConnector> =
        SlackEventsAxumListener::new(listener_environment.clone());

    // Build application route with OAuth nested router and Push/Command/Interaction events
    let app = axum::routing::Router::new()
        .nest(
            "/auth",
            listener.oauth_router("/auth", &oauth_listener_config, oauth_install_function),
        )
        .route("/installed", axum::routing::get(welcome_installed))
        .route("/cancelled", axum::routing::get(cancelled_install))
        .route("/error", axum::routing::get(error_install))
        .route(
            "/push",
            axum::routing::post(push_event).layer(
                listener
                    .events_layer(&signing_secret)
                    .with_event_extractor(SlackEventsExtractors::push_event()),
            ),
        )
        .route(
            "/command",
            axum::routing::post(command_event).layer(
                listener
                    .events_layer(&signing_secret)
                    .with_event_extractor(SlackEventsExtractors::command_event()),
            ),
        )
        .route(
            "/interaction",
            axum::routing::post(interaction_event).layer(
                listener
                    .events_layer(&signing_secret)
                    .with_event_extractor(SlackEventsExtractors::interaction_event()),
            ),
        );

    axum::serve(TcpListener::bind(&addr).await.unwrap(), app)
        .await
        .unwrap();

    Ok(())
}
