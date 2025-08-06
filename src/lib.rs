use async_io::Async;
use axum::handler::Handler;
use axum::response::IntoResponse;
use axum::routing::{MethodRouter, Route};
use axum::Router;
use bevy_defer::{AsyncExtension, AsyncPlugin, AsyncWorld};
use bevy_derive::{Deref, DerefMut};
use bevy_ecs::prelude::*;
use bevy_log::error;
use bevy_tasks::futures_lite::ready;
use hyper::server::conn::http1;
use hyper::Request;
use never_type::Never;
use smol_hyper::rt::{FuturesIo, SmolTimer};
use std::convert::Infallible;
use std::future::Future;
use std::net::{IpAddr, Ipv4Addr, TcpListener};
use std::pin::Pin;
use std::str::FromStr;
use std::task::{Context, Poll};
use tower::Layer;
use tower_service::Service;

pub struct BevyWebServerPlugin;

impl bevy_app::Plugin for BevyWebServerPlugin {
    fn build(&self, app: &mut bevy_app::App) {
        if !app.is_plugin_added::<AsyncPlugin>() {
            app.add_plugins(AsyncPlugin::default_settings());
        }
        app.add_systems(bevy_app::Startup, start_server);
    }
}

#[derive(Resource, Clone)]
pub struct WebServerConfig {
    ip: IpAddr,
    port: u16,
}

impl WebServerConfig {
    pub fn new(ip: IpAddr, port: u16) -> Self {
        Self { ip, port }
    }
}

impl Default for WebServerConfig {
    fn default() -> Self {
        Self {
            ip: IpAddr::V4(Ipv4Addr::from_str("127.0.0.1").unwrap()),
            port: 8080,
        }
    }
}

fn start_server(world: &mut bevy_ecs::world::World) {
    let web_server_config = world
        .remove_resource::<WebServerConfig>()
        .unwrap_or_default();
    world.spawn_task(async move {
        let Err(err) = server_main(web_server_config).await;
        error!("bevy_webserver failed with: {err}");
        Ok(())
    });
}

async fn server_main(
    WebServerConfig { ip, port }: WebServerConfig,
) -> Result<Never, anyhow::Error> {
    listen(Async::<TcpListener>::bind((ip, port))?).await
}
async fn listen(listener: Async<TcpListener>) -> Result<Never, anyhow::Error> {
    let router_wrapper: RouterWrapper = AsyncWorld
        .run(|world| -> RouterWrapper { world.remove_resource::<RouterWrapper>().unwrap() });
    let router = router_wrapper.0;
    let service = router.into_service();
    let service = TowerToHyperService { service };
    loop {
        let service = service.clone();
        let (client, _) = listener.accept().await?;
        AsyncWorld
            .spawn_task(async {
                match http1::Builder::new()
                    .timer(SmolTimer::new())
                    .serve_connection(FuturesIo::new(client), service)
                    .await
                {
                    Ok(_) => {}
                    Err(err) => error!("unable to server connection for bevy_webserver: {}", err),
                }
            })
            .detach();
        AsyncWorld.yield_now().await;
    }
}

/// This is left public in case you really wanna mess with it but it is not the intended api
#[derive(Resource, Default, Deref, DerefMut)]
pub struct RouterWrapper(pub Router);

pub trait RouterAppExt {
    fn router(&mut self, router_fn: impl FnOnce(Router) -> Router);
    fn route(&mut self, path: &str, method_router: MethodRouter<()>) -> &mut Self;
    fn route_service<T>(&mut self, path: &str, service: T) -> &mut Self
    where
        T: Service<axum::extract::Request, Error = Infallible> + Clone + Send + Sync + 'static,
        T::Response: IntoResponse,
        T::Future: Send + 'static;
    fn nest(&mut self, path: &str, router2: Router<()>) -> &mut Self;
    fn nest_service<T>(&mut self, path: &str, service: T) -> &mut Self
    where
        T: Service<axum::extract::Request, Error = Infallible> + Clone + Send + Sync + 'static,
        T::Response: IntoResponse,
        T::Future: Send + 'static;
    fn merge<R>(&mut self, other: R) -> &mut Self
    where
        R: Into<Router<()>>;
    fn layer<L>(&mut self, layer: L) -> &mut Self
    where
        L: Layer<Route> + Clone + Send + Sync + 'static,
        L::Service: Service<axum::extract::Request> + Clone + Send + Sync + 'static,
        <L::Service as Service<axum::extract::Request>>::Response: IntoResponse + 'static,
        <L::Service as Service<axum::extract::Request>>::Error: Into<Infallible> + 'static,
        <L::Service as Service<axum::extract::Request>>::Future: Send + 'static;
    fn route_layer<L>(&mut self, layer: L) -> &mut Self
    where
        L: Layer<Route> + Clone + Send + Sync + 'static,
        L::Service: Service<axum::extract::Request> + Clone + Send + Sync + 'static,
        <L::Service as Service<axum::extract::Request>>::Response: IntoResponse + 'static,
        <L::Service as Service<axum::extract::Request>>::Error: Into<Infallible> + 'static,
        <L::Service as Service<axum::extract::Request>>::Future: Send + 'static;
    fn fallback<H, T>(&mut self, handler: H) -> &mut Self
    where
        H: Handler<T, ()>,
        T: 'static;
    fn fallback_service<T>(&mut self, service: T) -> &mut Self
    where
        T: Service<axum::extract::Request, Error = Infallible> + Clone + Send + Sync + 'static,
        T::Response: IntoResponse,
        T::Future: Send + 'static;
    fn method_not_allowed_fallback<H, T>(&mut self, handler: H) -> &mut Self
    where
        H: Handler<T, ()>,
        T: 'static;
}

impl RouterAppExt for bevy_app::App {
    fn router(&mut self, router_fn: impl FnOnce(Router) -> Router) {
        self.world_mut().init_resource::<RouterWrapper>();
        if !self.is_plugin_added::<BevyWebServerPlugin>() {
            self.add_plugins(BevyWebServerPlugin);
        }
        let router = self
            .world_mut()
            .remove_resource::<RouterWrapper>()
            .unwrap()
            .0;
        self.world_mut()
            .insert_resource(RouterWrapper(router_fn(router)))
    }

    fn route(&mut self, path: &str, method_router: MethodRouter<()>) -> &mut Self {
        self.router(move |router| router.route(path, method_router));
        self
    }

    fn route_service<T>(&mut self, path: &str, service: T) -> &mut Self
    where
        T: Service<axum::extract::Request, Error = Infallible> + Clone + Send + Sync + 'static,
        T::Response: IntoResponse,
        T::Future: Send + 'static,
    {
        self.router(move |router| router.route_service(path, service));
        self
    }

    fn nest(&mut self, path: &str, router2: Router<()>) -> &mut Self {
        self.router(move |router| router.nest(path, router2));
        self
    }

    fn nest_service<T>(&mut self, path: &str, service: T) -> &mut Self
    where
        T: Service<axum::extract::Request, Error = Infallible> + Clone + Send + Sync + 'static,
        T::Response: IntoResponse,
        T::Future: Send + 'static,
    {
        self.router(|router| router.nest_service(path, service));
        self
    }

    fn merge<R>(&mut self, other: R) -> &mut Self
    where
        R: Into<Router<()>>,
    {
        self.router(|router| router.merge(other));
        self
    }

    fn layer<L>(&mut self, layer: L) -> &mut Self
    where
        L: Layer<Route> + Clone + Send + Sync + 'static,
        L::Service: Service<axum::extract::Request> + Clone + Send + Sync + 'static,
        <L::Service as Service<axum::extract::Request>>::Response: IntoResponse + 'static,
        <L::Service as Service<axum::extract::Request>>::Error: Into<Infallible> + 'static,
        <L::Service as Service<axum::extract::Request>>::Future: Send + 'static,
    {
        self.router(|router| router.layer(layer));
        self
    }

    fn route_layer<L>(&mut self, layer: L) -> &mut Self
    where
        L: Layer<Route> + Clone + Send + Sync + 'static,
        L::Service: Service<axum::extract::Request> + Clone + Send + Sync + 'static,
        <L::Service as Service<axum::extract::Request>>::Response: IntoResponse + 'static,
        <L::Service as Service<axum::extract::Request>>::Error: Into<Infallible> + 'static,
        <L::Service as Service<axum::extract::Request>>::Future: Send + 'static,
    {
        self.router(|router| router.layer(layer));
        self
    }

    fn fallback<H, T>(&mut self, handler: H) -> &mut Self
    where
        H: Handler<T, ()>,
        T: 'static,
    {
        self.router(|router| router.fallback(handler));
        self
    }

    fn fallback_service<T>(&mut self, service: T) -> &mut Self
    where
        T: Service<axum::extract::Request, Error = Infallible> + Clone + Send + Sync + 'static,
        T::Response: IntoResponse,
        T::Future: Send + 'static,
    {
        self.router(|router| router.fallback_service(service));
        self
    }

    fn method_not_allowed_fallback<H, T>(&mut self, handler: H) -> &mut Self
    where
        H: Handler<T, ()>,
        T: 'static,
    {
        self.router(|router| router.method_not_allowed_fallback(handler));
        self
    }
}

#[derive(Debug, Copy, Clone)]
pub struct TowerToHyperService<S> {
    pub service: S,
}

impl<S> hyper::service::Service<axum::extract::Request<hyper::body::Incoming>>
    for TowerToHyperService<S>
where
    S: tower_service::Service<axum::extract::Request> + Clone,
{
    type Response = S::Response;
    type Error = S::Error;
    type Future = Oneshot<S, axum::extract::Request>;

    fn call(&self, req: Request<hyper::body::Incoming>) -> Self::Future {
        let req = req.map(axum::body::Body::new);
        Oneshot::NotReady {
            svc: self.service.clone(),
            req: Some(req),
        }
    }
}

pin_project_lite::pin_project! {
    #[project = OneshotProj]
    pub enum Oneshot<S, R>
    where
        S: tower_service::Service<R>,
    {
        // We are not yet ready.
        NotReady {
            svc: S,
            req: Option<R>
        },
        // We have been called and are processing the request.
        Called {
            #[pin]
            fut: S::Future,
        },
        // We are done.
        Done
    }
}

impl<S, R> Future for Oneshot<S, R>
where
    S: tower_service::Service<R>,
{
    type Output = Result<S::Response, S::Error>;

    #[inline]
    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        loop {
            match self.as_mut().project() {
                OneshotProj::NotReady { svc, req } => {
                    ready!(svc.poll_ready(cx))?;
                    let fut = svc.call(req.take().expect("already called"));
                    self.as_mut().set(Oneshot::Called { fut });
                }

                OneshotProj::Called { fut } => {
                    let res = ready!(fut.poll(cx))?;
                    self.as_mut().set(Oneshot::Done);
                    return Poll::Ready(Ok(res));
                }

                OneshotProj::Done => panic!("future polled after completion"),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_new_web_server_config() {
        let web_server_config =
            WebServerConfig::new(IpAddr::V4(Ipv4Addr::from_str("127.0.0.1").unwrap()), 8080);
        assert_eq!(web_server_config.ip, Ipv4Addr::new(127, 0, 0, 1));
        assert_eq!(web_server_config.port, 8080);
    }
}
