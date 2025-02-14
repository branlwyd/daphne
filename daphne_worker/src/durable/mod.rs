// Copyright (c) 2022 Cloudflare, Inc. All rights reserved.
// SPDX-License-Identifier: BSD-3-Clause

pub(crate) mod aggregate_store;
pub(crate) mod garbage_collector;
pub(crate) mod helper_state_store;

use crate::{
    int_err, now,
    tracing_utils::{shorten_paths, DaphneSubscriber, JsonFields},
    DapWorkerMode,
};
use daphne::messages::TaskId;
use daphne_service_utils::{
    config::DaphneWorkerDeployment,
    durable_requests::bindings::{self, DurableMethod, GarbageCollector},
};
use rand::prelude::*;
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use std::{cmp::min, time::Duration};
use tracing::{info_span, trace, warn};
use worker::{
    async_trait, js_sys::Uint8Array, Delay, Env, Error, Headers, ListOptions, Method, Request,
    RequestInit, Result, ScheduledTime, State, Stub,
};

const ERR_NO_VALUE: &str = "No such value in storage.";

// The maximum number of keys to get at once in a list command.
//
// The DO API does not say that there is any limit on the number of keys it is willing to return
// other than overall DO/worker memory, which it warns you not to exceed. Imposing some sort of
// limit is a good idea, lest we get DoS'd by a task configuration with a large value.
//
// Currently the value is set to 128, as the miniflare environment will fail with more than this.
// This appears to be a miniflare bug and not part of the API.
//
// We have not been able to replicate failures with wrangler2 in local or experimental-local mode.
//
// TODO(bhalley) does this need to be configurable?
const MAX_KEYS: usize = 128;

const RETRY_DELAYS: &[Duration] = &[
    Duration::from_millis(100),
    Duration::from_millis(500),
    Duration::from_millis(1_000),
    Duration::from_millis(3_000),
];

/// Used to send HTTP requests to a durable object (DO) instance.
pub(crate) struct DurableConnector<'srv> {
    env: &'srv Env,
    retry: bool,
}

impl<'srv> DurableConnector<'srv> {
    pub(crate) fn new(env: &'srv Env) -> Self {
        DurableConnector { env, retry: false }
    }

    /// Send a POST request with the given path to the DO instance with the given binding and name.
    /// The body of the request is a JSON object. The response is expected to be a JSON object.
    pub(crate) async fn post<I: Serialize, O: for<'b> Deserialize<'b>>(
        &self,
        durable_binding: &str,
        durable_path: &'static str,
        durable_name: String,
        data: I,
    ) -> Result<O> {
        self.post_with_handler(
            durable_binding,
            durable_path,
            durable_name,
            data,
            |output, _retried| output,
        )
        .await
    }

    /// Like `post()`, except `handler` is called on the result. The callback is given an
    /// indication of whether the request was retried.
    pub(crate) async fn post_with_handler<I, O1, O2, H>(
        &self,
        durable_binding: &str,
        durable_path: &'static str,
        durable_name: String,
        data: I,
        handler: H,
    ) -> Result<O2>
    where
        I: Serialize,
        O1: for<'b> Deserialize<'b>,
        H: FnOnce(O1, bool) -> O2 + Sized,
    {
        let namespace = self.env.durable_object(durable_binding)?;
        let stub = namespace.id_from_name(&durable_name)?.get_stub()?;
        self.durable_request(
            stub,
            durable_binding,
            durable_path,
            Method::Post,
            Some(data),
            handler,
        )
        .await
        .map_err(|error| {
            Error::RustError(format!(
                "DO {durable_binding}: post {durable_path}: {error}"
            ))
        })
    }

    /// Send a POST request with the given path to the DO instance with the given binding and hex
    /// identifier. The body of the request is a JSON object. The response is expected to be a JSON
    /// object.
    pub(crate) async fn post_by_id_hex<I: Serialize, O: for<'b> Deserialize<'b>>(
        &self,
        durable_binding: &str,
        durable_path: &'static str,
        durable_id_hex: String,
        data: I,
    ) -> Result<O> {
        let namespace = self.env.durable_object(durable_binding)?;
        let stub = namespace.id_from_string(&durable_id_hex)?.get_stub()?;
        self.durable_request(
            stub,
            durable_binding,
            durable_path,
            Method::Post,
            Some(data),
            |output, _retried| output,
        )
        .await
        .map_err(|error| {
            Error::RustError(format!(
                "DO {durable_binding}: post {durable_path}: {error}"
            ))
        })
    }

    async fn durable_request<I, O1, O2, H>(
        &self,
        durable_stub: Stub,
        durable_binding: &str,
        durable_path: &'static str,
        method: Method,
        data: Option<I>,
        handler: H,
    ) -> Result<O2>
    where
        I: Serialize,
        O1: for<'a> Deserialize<'a>,
        H: FnOnce(O1, bool) -> O2 + Sized,
    {
        let attempts = if self.retry {
            RETRY_DELAYS.len() + 1
        } else {
            1
        };

        let tracing_headers = span_to_headers();

        let mut attempt = 1;
        loop {
            let req = match (&method, &data) {
                (Method::Post, Some(data)) => {
                    let data = bincode::serialize(&data).map_err(|e| {
                        Error::RustError(format!("failed to serialize data: {e:?}"))
                    })?;
                    let buffer =
                        Uint8Array::new_with_length(data.len().try_into().map_err(|_| {
                            worker::Error::RustError(format!("buffer is too long {}", data.len()))
                        })?);
                    buffer.copy_from(&data);
                    Request::new_with_init(
                        &format!("https://fake-host{durable_path}"),
                        RequestInit::new()
                            .with_method(Method::Post)
                            .with_body(Some(buffer.into()))
                            .with_headers(tracing_headers.clone()),
                    )?
                }
                (Method::Get, None) => Request::new_with_init(
                    &format!("https://fake-host{durable_path}"),
                    RequestInit::new()
                        .with_method(Method::Get)
                        .with_headers(tracing_headers.clone()),
                )?,
                _ => {
                    return Err(Error::RustError(format!(
                        "durable_request: Unrecognized method: {method:?}",
                    )));
                }
            };

            match durable_stub.fetch_with_request(req).await {
                Ok(mut resp) => return Ok(handler(resp.json().await?, attempt > 1)),
                Err(err) => {
                    if attempt < attempts {
                        warn!("DO {durable_binding}: post {durable_path}: attempt #{attempt} failed: {err}");
                        Delay::from(RETRY_DELAYS[attempt - 1]).await;
                        attempt += 1;
                    } else {
                        return Err(err);
                    }
                }
            }
        }
    }
}

trait DapDurableObject {
    type DurableMethod: DurableMethod;

    fn state(&self) -> &State;

    fn deployment(&self) -> DaphneWorkerDeployment;
}

#[async_trait::async_trait(?Send)]
trait Alarmed: DapDurableObject {
    /// A mutable property used to track whether this DO has been alarmed.
    fn alarmed(&mut self) -> &mut bool;

    /// Ensure the alarm call is setup for this DO.
    async fn ensure_alarmed<L: Into<ScheduledTime>>(&mut self, lifetime: L) -> Result<()> {
        if !*self.alarmed() {
            let result = self.state().storage().get_alarm().await;
            match result {
                Ok(None) => {
                    self.state().storage().set_alarm(lifetime).await?;
                }
                Ok(Some(_)) => { /* alarm already setup */ }
                Err(e) => {
                    if matches!(self.deployment(), DaphneWorkerDeployment::Dev) {
                        warn!("ignoring get_alarm() failure in a dev environment until --experimental-local implements it: {e}");
                    } else {
                        // We only return an error if not in the "dev" deployment as
                        // the experimental-local dev environment doesn't have
                        // working get_alarm() and set_alarm() yet, so we want to
                        // ignore errors in that case.
                        return Err(e);
                    }
                }
            }
            *self.alarmed() = true;
        }
        Ok(())
    }
}

#[async_trait::async_trait(?Send)]
trait GarbageCollectable: DapDurableObject {
    /// A mutable property used to track whether this DO has been touched.
    fn touched(&mut self) -> &mut bool;

    fn env(&self) -> &Env;

    /// Run garbage collection requests.
    ///
    /// If a garbage collection request is handled no further processing needs to be done, as such,
    /// this function will consume the request, otherwise it will return the passed in request for
    /// further handling.
    async fn schedule_for_garbage_collection(
        &mut self,
        req: Request,
    ) -> Result<std::ops::ControlFlow<(), Request>> {
        match GarbageCollector::try_from_uri(&req.path()) {
            Some(GarbageCollector::DeleteAll) => {
                self.state().storage().delete_all().await?;
                *self.touched() = false;
                return Ok(std::ops::ControlFlow::Break(()));
            }
            _ if !*self.touched() => {
                // The GarbageCollector should only be used when running tests. In production, the DO->DO
                // communication overhead adds unacceptable latency, and there's no need to do the
                // bulk deletes of state that test suites require.
                if matches!(self.deployment(), DaphneWorkerDeployment::Dev) {
                    let touched = state_set_if_not_exists(self.state(), "touched", &true)
                        .await?
                        .unwrap_or(false);
                    if !touched {
                        let durable = crate::durable::DurableConnector::new(self.env());
                        durable
                            .post(
                                bindings::GarbageCollector::BINDING,
                                bindings::GarbageCollector::Put.to_uri(),
                                bindings::GarbageCollector::name(()).unwrap_from_name(),
                                &crate::durable::DurableReference {
                                    binding: Self::DurableMethod::BINDING.to_string(),
                                    id_hex: self.state().id().to_string(),
                                    task_id: None,
                                },
                            )
                            .await?;
                    }
                }
                *self.touched() = true;
            }
            _ => {}
        }
        Ok(std::ops::ControlFlow::Continue(req))
    }
}

/// Fetch the value associated with the given key from durable storage. If the key/value pair does
/// not exist, then return the default value.
pub(crate) async fn state_get_or_default<T: Default + for<'a> Deserialize<'a>>(
    state: &State,
    key: &str,
) -> Result<T> {
    state.storage().get(key).await.or_else(|e| {
        if matches!(e, Error::JsError(ref s) if s == ERR_NO_VALUE) {
            Ok(T::default())
        } else {
            Err(e)
        }
    })
}

pub(crate) async fn state_get<T: for<'a> Deserialize<'a>>(
    state: &State,
    key: &str,
) -> Result<Option<T>> {
    state.storage().get(key).await.or_else(|e| {
        if matches!(e, Error::JsError(ref s) if s == ERR_NO_VALUE) {
            Ok(None)
        } else {
            Err(e)
        }
    })
}

/// Set a key/value pair unless the key already exists. If the key exists, then return the current
/// value. Otherwise return nothing.
pub(crate) async fn state_set_if_not_exists<T: for<'a> Deserialize<'a> + Serialize>(
    state: &State,
    key: &str,
    val: &T,
) -> Result<Option<T>> {
    let curr_val: Option<T> = state_get(state, key).await?;
    if curr_val.is_some() {
        return Ok(curr_val);
    }

    state.storage().put(key, val).await?;
    Ok(None)
}

/// Reference to a DO instance, used by the garbage collector.
#[derive(Deserialize, Serialize)]
pub(crate) struct DurableReference {
    /// The DO binding, e.g., "DAP_REPORT_STORE".
    pub(crate) binding: String,

    /// Unique ID assigned to the DO instance by the Workers runtime.
    pub(crate) id_hex: String,

    /// If applicable, the DAP task ID to which the DO instance is associated.
    pub(crate) task_id: Option<TaskId>,
}

/// An element of a queue stored in a DO instance.
#[derive(Deserialize, Serialize)]
pub(crate) struct DurableOrdered<T> {
    item: T,
    prefix: String,
    ordinal: String,
}

impl<T: for<'a> Deserialize<'a> + Serialize> DurableOrdered<T> {
    /// Return all elements in the queue.
    ///
    /// WARNING: If the queue is too long, then this action is likely to cause the Workers runtime
    /// to start rate limiting the Worker. This should only be used when the size of the queue is
    /// strictly controlled.
    async fn get_all(state: &State, prefix: &str) -> Result<Vec<Self>> {
        get_front(state, prefix, None).await
    }

    /// Create a new element for a roughly ordered queue. (Use `put()` to store it.)
    ///
    /// Items in this queue are handled roughly in order of creation (oldest elements first).
    /// Specifically, the ordinal is the UNIX time (in seconds) at which this method was called.
    /// Ties are broken by a random nonce tacked on to the key. The format of the ordinal is:
    ///
    /// ```text
    ///     time/<time>/nonce/<nonce>
    /// ```
    ///
    /// where <time> is the timestamp and <nonce> is a random nonce.
    pub(crate) fn new_roughly_ordered(item: T, prefix: &str) -> Self {
        let mut rng = thread_rng();
        let time = now();
        let nonce = rng.gen::<[u8; 16]>();

        // Pad the timestamp with 0s to the length of the longest 64-bit integer encoded in
        // decimal. This ensures that queue elements stay ordered.
        let ordinal = format!("time/{:020}/nonce/{}", time, hex::encode(nonce));

        Self {
            item,
            prefix: prefix.to_string(),
            ordinal,
        }
    }

    /// Store the item in the provided DO state.
    pub(crate) async fn put(&self, state: &State) -> Result<()> {
        state.storage().put(&self.key(), &self.item).await
    }

    /// Compute the key used to store store the item. The key format is:
    ///
    /// ```text
    ///     <prefix>/item/<ordinal>
    /// ```
    ///
    /// where `<prefix>` is the indicated namespace and `<ordinal>` is the item's ordinal.
    pub(crate) fn key(&self) -> String {
        format!("{}/item/{}", self.prefix, self.ordinal)
    }
}

impl<T> AsRef<T> for DurableOrdered<T> {
    fn as_ref(&self) -> &T {
        &self.item
    }
}

pub(crate) struct DaphneWorkerDurableConfig {
    /// Deployment type. This controls certain behavior overrides relevant to specific deployments.
    pub deployment: DaphneWorkerDeployment,

    /// Helper: Time to wait before deleting an instance of HelperStateStore. This field is not
    /// configured by the Leader.
    pub helper_state_store_garbage_collect_after_secs: Option<Duration>,
}

impl DaphneWorkerDurableConfig {
    pub(crate) fn from_worker_env(env: &Env) -> Result<Self> {
        let is_do_proxy = crate::get_worker_mode(env) == DapWorkerMode::StorageProxy;

        let is_leader = match env.var("DAP_AGGREGATOR_ROLE").map(|s| s.to_string()) {
            Ok(r) if r == "leader" => Some(true),
            Ok(r) if r == "helper" => Some(false),
            Err(_) if is_do_proxy => None,
            other => {
                let other = other?;
                return Err(worker::Error::RustError(format!(
                    "Invalid value for DAP_AGGREGATOR_ROLE: '{other}'",
                )));
            }
        };

        let deployment = if let Ok(deployment) = env.var("DAP_DEPLOYMENT") {
            match deployment.to_string().as_str() {
                "prod" => DaphneWorkerDeployment::Prod,
                "dev" => DaphneWorkerDeployment::Dev,
                s => {
                    return Err(worker::Error::RustError(format!(
                        "Invalid value for DAP_DEPLOYMENT: {s}",
                    )))
                }
            }
        } else {
            DaphneWorkerDeployment::default()
        };
        if !matches!(deployment, DaphneWorkerDeployment::Prod) {
            trace!("DAP deployment override applied: {deployment:?}");
        }

        let helper_state_store_garbage_collect_after_secs = if let Some(false) | None = is_leader {
            Some(Duration::from_secs(
                env.var("DAP_HELPER_STATE_STORE_GARBAGE_COLLECT_AFTER_SECS")?
                    .to_string()
                    .parse()
                    .map_err(|err| {
                        worker::Error::RustError(format!(
                            "Failed to parse DAP_HELPER_STATE_STORE_GARBAGE_COLLECT_AFTER_SECS: {err}"
                        ))
                    })?,
            ))
        } else {
            None
        };

        Ok(Self {
            deployment,
            helper_state_store_garbage_collect_after_secs,
        })
    }
}

async fn get_front<T: for<'a> Deserialize<'a> + Serialize>(
    state: &State,
    prefix: &str,
    limit: Option<usize>,
) -> Result<Vec<DurableOrdered<T>>> {
    let key_prefix = format!("{prefix}/item/");
    let mut opt = ListOptions::new().prefix(&key_prefix);
    if let Some(limit) = limit {
        // Note we impose an upper limit on the user's specified limit.
        opt = opt.limit(min(limit, MAX_KEYS));
    }
    let iter = state.storage().list_with_options(opt).await?.entries();
    let mut js_item = iter.next()?;
    let mut res = Vec::new();
    while !js_item.done() {
        let (key, item): (String, T) =
            serde_wasm_bindgen::from_value(js_item.value()).map_err(int_err)?;
        if key[..key_prefix.len()] != key_prefix {
            return Err(int_err("queue element key is improperly formatted"));
        }
        let ordinal = &key[key_prefix.len()..];
        res.push(DurableOrdered {
            item,
            prefix: prefix.to_string(),
            ordinal: ordinal.to_string(),
        });
        js_item = iter.next()?;
    }
    Ok(res)
}

fn span_to_headers() -> Headers {
    // get the current span.
    let span = tracing::Span::current();

    // get the current global subscriber
    tracing::dispatcher::get_default(|d| {
        use tracing_subscriber::registry::LookupSpan;

        // downcast it to our subscriber
        let Some(sub) = d.downcast_ref::<DaphneSubscriber>() else {
            return Default::default();
        };

        // get the span id, so we can ask the subscriber for the current span
        let Some(id) = span.id() else {
            return Default::default();
        };

        let mut headers = Headers::default();

        // loop over the stack of spans, starting with the current one and going up.
        for span_ref in std::iter::successors(sub.span(&id), |span| span.parent()) {
            // get the json fields extension provided by the [JsonFieldsLayer].
            let ext = span_ref.extensions();
            let Some(fields) = ext.get::<JsonFields>() else {
                continue;
            };

            for (k, v) in fields {
                let non_string_stack_slot: String;
                let (k, v) = (
                    // prepend "tracing-" to all the headers to avoid accidental collisions.
                    format!("tracing-{k}"),
                    match v {
                        serde_json::Value::String(s) => s,
                        v => {
                            non_string_stack_slot = v.to_string();
                            &non_string_stack_slot
                        }
                    },
                );
                if matches!(headers.has(&k), Ok(false)) {
                    if let Err(e) = headers.append(&k, v) {
                        tracing::warn!(
                            error = %e,
                            key = %k,
                            "invalid name passed to headers"
                        );
                    }
                }
            }
        }

        headers
    })
}

async fn req_parse<T: DeserializeOwned>(req: &mut Request) -> Result<T> {
    let bytes = req.bytes().await?;
    bincode::deserialize(&bytes)
        .map_err(|e| Error::RustError(format!("failed to deserialize bincode: {e:?}")))
}

fn create_span_from_request(req: &Request) -> tracing::Span {
    let path = req.path();
    let span = info_span!("DO span", p = %shorten_paths(path.split('/')).display());
    span.in_scope(|| tracing::info!("{}", path));
    span
}
