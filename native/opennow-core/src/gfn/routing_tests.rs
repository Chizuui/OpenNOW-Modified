use super::tests::{auth_fixture, jwt, mock_requests, test_service};
use super::*;
use std::sync::atomic::{AtomicUsize, Ordering};

fn service(url: &str) -> (GfnService, PathBuf) {
    let (mut service, path) = test_service(url);
    service.endpoints.service_urls = format!("{url}/providers");
    service.endpoints.server_info = Some(format!("{url}/v2/serverInfo"));
    service.endpoints.graphql = format!("{url}/graphql");
    service.endpoints.subscription = format!("{url}/subscription");
    let mut state = service.state.lock().unwrap();
    state.session = Some(auth_fixture("account-a"));
    state.generation = 7;
    state.restore_attempted = true;
    drop(state);
    (service, path)
}

fn directory(id: &str, host: &str) -> Value {
    json!({"gfnServiceInfo":{"gfnServiceEndpoints":[{
        "idpId":id,"loginProviderCode":"ALLIANCE","loginProviderDisplayName":"Alliance fixture",
        "streamingServiceUrl":format!("https://{host}/"),"loginProviderPriority":1
    }]}})
}

fn expire_discovery(service: &GfnService) {
    let mut state = service.state.lock().unwrap();
    state.providers_expires = None;
    state.providers_retry = None;
}

fn reject_concurrent_create_during(discovery: bool) {
    let (entered_tx, entered_rx) = std::sync::mpsc::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let (url, worker) = mock_requests(vec![(503, json!({}))], move |_, request| {
        if discovery {
            assert!(request.starts_with("GET /providers "));
        } else {
            assert!(request.starts_with("POST /v2/session?"));
        }
        entered_tx.send(()).unwrap();
        release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
    });
    let (mut service, path) = service(&url);
    service
        .cloudmatch
        .set_test_control_base(url::Url::parse(&url).unwrap());
    if discovery {
        expire_discovery(&service);
    }
    let requests = std::sync::Arc::new(crate::requests::Requests::default());
    let permit = requests.admit("first-create", "session.create").unwrap();
    std::thread::scope(|threads| {
        let first = threads.spawn(|| {
            crate::requests::scope(permit.token.clone(), || {
                service.create_session(&json!({"appId":"123"}), &json!({}))
            })
        });
        entered_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        let (result_tx, result_rx) = std::sync::mpsc::channel();
        let service = &service;
        let second = threads.spawn(move || {
            result_tx
                .send(service.create_session(&json!({"appId":"456"}), &json!({})))
                .unwrap()
        });
        let rejected = result_rx.recv_timeout(Duration::from_secs(1));
        if discovery {
            requests.cancel("first-create");
        }
        release_tx.send(()).unwrap();
        assert_eq!(
            first.join().unwrap().unwrap_err().code,
            if discovery {
                "cancelled"
            } else {
                "upstream_error"
            }
        );
        assert_eq!(
            rejected
                .expect("concurrent RPC queued behind the first create")
                .unwrap_err()
                .code,
            "session_update_busy"
        );
        second.join().unwrap();
    });
    assert!(service.cloudmatch.active()["session"].is_null());
    assert!(service.cloudmatch.admit_create().is_ok());
    worker.join().unwrap();
    std::fs::remove_dir_all(path).unwrap();
}

#[test]
fn concurrent_rpc_create_is_rejected_before_cancelled_provider_preflight_finishes() {
    reject_concurrent_create_during(true);
}

#[test]
fn concurrent_rpc_create_is_rejected_before_failed_allocation_post_finishes() {
    reject_concurrent_create_during(false);
}

#[test]
fn stopping_same_owner_renews_expired_service_id_before_exactly_one_delete() {
    for status in [204, 401, 503] {
        let renewed_id = jwt("account-a", now_ms() + 3_600_000);
        let expected_id = renewed_id.clone();
        let delete_count = std::sync::Arc::new(AtomicUsize::new(0));
        let observed = delete_count.clone();
        let (url, worker) = mock_requests(
            vec![
                (
                    200,
                    json!({"access_token":"renewed-access","id_token":renewed_id,"expires_in":3600}),
                ),
                (status, json!({})),
            ],
            move |index, request| {
                if index == 0 {
                    assert!(request.starts_with("POST / "));
                    assert!(request.contains("client_id=test-client-id"));
                } else {
                    assert!(request.starts_with("DELETE /v2/session/owned-seat HTTP/1.1"));
                    assert!(request.contains(&format!("GFNJWT {expected_id}")));
                    assert!(!request.contains("expired-service-id"));
                    observed.fetch_add(1, Ordering::SeqCst);
                }
            },
        );
        let (mut service, path) = service(&url);
        let mut owner = auth_fixture("account-a");
        owner.tokens.id_token = Some("expired-service-id".into());
        owner.tokens.id_token_expires_at = Some(now_ms() - 1);
        service.state.lock().unwrap().session = Some(owner.clone());
        service.session_routing.lock().unwrap().active_owner = Some((owner, 7));
        service
            .cloudmatch
            .set_test_control_base(url::Url::parse(&url).unwrap());
        service
            .cloudmatch
            .seed_owned_session(json!({"sessionId":"owned-seat","status":3}));
        let result = service.stop_session(
            &json!({"sessionId":"owned-seat","streamingBaseUrl":"https://foreign.nvidiagrid.net/"}),
            &json!({}),
        );
        if status == 204 {
            assert_eq!(result.unwrap()["scope"]["userId"], "account-a");
            assert!(
                service
                    .session_routing
                    .lock()
                    .unwrap()
                    .active_owner
                    .is_none()
            );
        } else {
            assert_eq!(
                result.unwrap_err().code,
                if status == 401 {
                    "http_unauthorized"
                } else {
                    "upstream_error"
                }
            );
            assert_eq!(
                service.cloudmatch.active()["session"]["sessionId"],
                "owned-seat"
            );
        }
        worker.join().unwrap();
        assert_eq!(delete_count.load(Ordering::SeqCst), 1);
        std::fs::remove_dir_all(path).unwrap();
    }
}

#[test]
fn stopping_expired_foreign_owner_never_renews_or_deletes_with_selected_credentials() {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.set_nonblocking(true).unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let (mut service, path) = service(&url);
    let mut owner = auth_fixture("account-a");
    owner.tokens.id_token = Some("expired-service-id".into());
    owner.tokens.id_token_expires_at = Some(now_ms() - 1);
    let mut selected = auth_fixture("account-b");
    selected.tokens.access_token = "foreign-access".into();
    service.state.lock().unwrap().session = Some(selected);
    service.state.lock().unwrap().generation = 8;
    service.session_routing.lock().unwrap().active_owner = Some((owner, 7));
    service
        .cloudmatch
        .set_test_control_base(url::Url::parse(&url).unwrap());
    service
        .cloudmatch
        .seed_owned_session(json!({"sessionId":"owned-seat","status":3}));
    assert_eq!(
        service
            .stop_session(&json!({"sessionId":"owned-seat"}), &json!({}))
            .unwrap_err()
            .code,
        "authentication_required"
    );
    assert_eq!(
        service
            .stop_session(&json!({"sessionId":"foreign-seat"}), &json!({}))
            .unwrap_err()
            .code,
        "session_owner_mismatch"
    );
    assert_eq!(
        listener.accept().unwrap_err().kind(),
        std::io::ErrorKind::WouldBlock
    );
    assert_eq!(
        service.cloudmatch.active()["session"]["sessionId"],
        "owned-seat"
    );
    std::fs::remove_dir_all(path).unwrap();
}

#[test]
fn discovery_transport_schema_and_untrusted_endpoints_remain_retryable_failures() {
    for payload in [
        json!("invalid discovery"),
        json!({"gfnServiceInfo":{"gfnServiceEndpoints":[]}}),
        directory("outside", "outside.invalid"),
    ] {
        let (url, worker) = mock_requests(vec![(200, payload)], |_, _| {});
        let (service, path) = service(&url);
        expire_discovery(&service);
        let result = service.providers().unwrap();
        assert_eq!(result["discovery"]["state"], "degraded");
        assert!(service.state.lock().unwrap().providers_expires.is_none());
        assert!(service.state.lock().unwrap().providers_retry.is_some());
        worker.join().unwrap();
        std::fs::remove_dir_all(path).unwrap();
    }
    let (service, path) = service("http://127.0.0.1:1");
    expire_discovery(&service);
    assert_eq!(
        service.providers().unwrap()["discovery"]["state"],
        "degraded"
    );
    std::fs::remove_dir_all(path).unwrap();
}

#[test]
fn discovery_honors_bounded_retry_after_and_rejects_malformed_json() {
    use std::io::{BufRead, BufReader, Write};
    for (status, body, retry) in [
        (429, "{}", "120".to_owned()),
        (
            429,
            "{}",
            httpdate::fmt_http_date(SystemTime::now() + Duration::from_secs(125)),
        ),
        (200, "{", String::new()),
    ] {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        let worker = std::thread::spawn(move || {
            let (mut stream, _) = listener.accept().unwrap();
            stream
                .set_read_timeout(Some(Duration::from_secs(5)))
                .unwrap();
            let mut reader = BufReader::new(&mut stream);
            loop {
                let mut line = String::new();
                assert!(reader.read_line(&mut line).unwrap() > 0);
                if line == "\r\n" {
                    break;
                }
            }
            write!(stream,"HTTP/1.1 {status} Test\r\nContent-Length: {}\r\nRetry-After: {retry}\r\nConnection: close\r\n\r\n{body}",body.len()).unwrap();
        });
        let (service, path) = service(&url);
        expire_discovery(&service);
        let result = service.providers().unwrap();
        assert_eq!(result["discovery"]["state"], "degraded");
        let delay = result["discovery"]["retryAfterMs"].as_u64().unwrap();
        assert!(delay <= 3_600_000);
        if status == 429 {
            assert!(delay > 115_000);
        } else {
            assert!(delay <= 30_000);
        }
        worker.join().unwrap();
        std::fs::remove_dir_all(path).unwrap();
    }
}

#[test]
fn library_and_vpc_use_the_same_explicit_proxy_without_direct_fallback() {
    let (proxy, worker) = mock_requests(
        vec![
            (200, json!({"requestStatus":{"serverId":"proxy-vpc"}})),
            (
                200,
                json!({"data":{"apps":{"items":[],"pageInfo":{"hasNextPage":false}}}}),
            ),
        ],
        |index, request| {
            if index == 0 {
                assert!(request.starts_with("GET http://metadata.fixture.invalid/v2/serverInfo "));
            } else {
                assert!(request.starts_with("POST http://catalog.fixture.invalid/graphql "));
                assert!(request.contains("proxy-vpc"));
            }
            assert!(request.contains("GFNJWT test-access"));
        },
    );
    let (mut service, path) = service("http://127.0.0.1:1");
    service.endpoints.server_info = Some("http://metadata.fixture.invalid/v2/serverInfo".into());
    service.endpoints.graphql = "http://catalog.fixture.invalid/graphql".into();
    let result = service
        .library_catalog(
            &json!({}),
            &json!({"sessionProxyEnabled":true,"sessionProxyUrl":proxy}),
        )
        .unwrap();
    assert_eq!(result["source"], "account-library");
    worker.join().unwrap();
    std::fs::remove_dir_all(path).unwrap();
}

#[test]
fn advertised_default_provider_does_not_replace_an_explicit_selection() {
    let mut payload = directory("alliance", "alliance.nvidiagrid.net");
    payload["gfnServiceInfo"]["defaultProvider"] = json!("ALLIANCE");
    let (url, worker) = mock_requests(vec![(200, payload)], |_, _| {});
    let (service, path) = service(&url);
    expire_discovery(&service);
    let result = service.providers().unwrap();
    assert_eq!(result["defaultProviderIdpId"], "alliance");
    assert_eq!(
        service
            .start_device_login(&json!({"providerIdpId":"deleted"}))
            .unwrap_err()
            .code,
        "provider_unavailable"
    );
    worker.join().unwrap();
    std::fs::remove_dir_all(path).unwrap();
}

#[test]
fn failed_discovery_is_degraded_and_retries_without_remapping_explicit_identity() {
    let (url, worker) = mock_requests(
        vec![
            (503, json!({})),
            (200, directory("alliance", "alliance.nvidiagrid.net")),
        ],
        |_, request| {
            assert!(request.starts_with("GET /providers "));
            assert!(!request.to_ascii_lowercase().contains("authorization:"));
        },
    );
    let (service, path) = service(&url);
    expire_discovery(&service);
    service.state.lock().unwrap().providers.clear();
    let first = service.providers().unwrap();
    assert_eq!(first["discovery"]["state"], "degraded");
    assert!(service.state.lock().unwrap().providers.is_empty());
    assert!(first["discovery"]["retryAfterMs"].as_u64().unwrap() > 0);
    assert_eq!(
        service.providers().unwrap()["discovery"]["state"],
        "degraded"
    );
    assert_eq!(
        service
            .start_device_login(&json!({"providerIdpId":"missing-alliance"}))
            .unwrap_err()
            .code,
        "provider_unavailable"
    );
    expire_discovery(&service);
    let recovered = service.providers().unwrap();
    assert_eq!(recovered["discovery"]["state"], "ready");
    assert_eq!(recovered["providers"][0]["idpId"], "alliance");
    worker.join().unwrap();
    std::fs::remove_dir_all(path).unwrap();
}

#[test]
fn discovery_reconciles_only_matching_provider_and_fences_changed_or_removed_routes() {
    let (url, worker) = mock_requests(
        vec![
            (503, json!({})),
            (200, directory("alliance", "new.nvidiagrid.net")),
            (200, directory("another", "other.nvidiagrid.net")),
        ],
        |_, _| {},
    );
    let (service, path) = service(&url);
    {
        let mut state = service.state.lock().unwrap();
        let session = state.session.as_mut().unwrap();
        session.provider.idp_id = "alliance".into();
        session.provider.streaming_service_url = "https://old.nvidiagrid.net/".into();
        state.providers.clear();
    }
    expire_discovery(&service);
    assert_eq!(
        service.providers().unwrap()["providers"][0]["idpId"],
        "alliance"
    );
    let (old, generation) = service
        .authenticated_snapshot(TokenPurpose::ServiceId, false)
        .unwrap();
    assert_eq!(
        old.provider.streaming_service_url,
        "https://old.nvidiagrid.net/"
    );
    expire_discovery(&service);
    service.providers().unwrap();
    assert_eq!(
        service.check_scope(&old, generation).unwrap_err().code,
        "stale_account"
    );
    let (new, generation) = service
        .authenticated_snapshot(TokenPurpose::ServiceId, false)
        .unwrap();
    assert_eq!(
        new.provider.streaming_service_url,
        "https://new.nvidiagrid.net/"
    );
    expire_discovery(&service);
    service.providers().unwrap();
    assert_eq!(
        service.check_scope(&new, generation).unwrap_err().code,
        "stale_account"
    );
    assert_eq!(
        service
            .authenticated_snapshot(TokenPurpose::ServiceId, false)
            .unwrap_err()
            .code,
        "provider_unavailable"
    );
    worker.join().unwrap();
    std::fs::remove_dir_all(path).unwrap();
}

#[test]
fn server_info_failures_never_dispatch_a_generic_library_query() {
    for (status, body, expected) in [
        (403, json!({}), "upstream_error"),
        (429, json!({}), "upstream_error"),
        (200, json!({}), "invalid_upstream_response"),
        (503, json!({}), "upstream_error"),
    ] {
        let (url, worker) = mock_requests(vec![(status, body)], |_, request| {
            assert!(request.starts_with("GET /v2/serverInfo "));
            assert!(request.contains("GFNJWT test-access"));
            assert!(!request.contains("GFNPartnerJWT"));
        });
        let (service, path) = service(&url);
        assert_eq!(
            service
                .library_catalog(&json!({}), &json!({}))
                .unwrap_err()
                .code,
            expected
        );
        worker.join().unwrap();
        std::fs::remove_dir_all(path).unwrap();
    }
}

#[test]
fn library_401_renews_once_and_replays_with_the_same_owner_and_new_vpc() {
    let (url, worker) = mock_requests(
        vec![
            (200, json!({"requestStatus":{"serverId":"vpc-old"}})),
            (401, json!({})),
            (
                200,
                json!({"access_token":"renewed-access","expires_in":3600}),
            ),
            (
                200,
                json!({"sub":"account-a","email":"fixture@example.invalid"}),
            ),
            (200, json!({"requestStatus":{"serverId":"vpc-renewed"}})),
            (
                200,
                json!({"data":{"apps":{"items":[],"pageInfo":{"totalCount":0,"hasNextPage":false}}}}),
            ),
        ],
        |index, request| {
            if matches!(index, 0 | 1 | 4 | 5) {
                let expected = if index < 2 {
                    "test-access"
                } else {
                    "renewed-access"
                };
                assert!(request.contains(&format!("GFNJWT {expected}")));
            }
            if index == 1 {
                assert!(request.starts_with("POST /graphql "));
                assert!(request.contains("vpc-old"));
            }
            if index == 5 {
                assert!(request.starts_with("POST /graphql "));
                assert!(request.contains("vpc-renewed"));
            }
            if index == 2 {
                assert!(request.contains("client_id=test-client-id"));
            }
        },
    );
    let (service, path) = service(&url);
    let result = service.library_catalog(&json!({}), &json!({})).unwrap();
    assert_eq!(result["scope"]["generation"], 7);
    assert_eq!(result["scope"]["userId"], "account-a");
    assert!(!result.to_string().contains("renewed-access"));
    worker.join().unwrap();
    std::fs::remove_dir_all(path).unwrap();
}

#[test]
fn second_401_is_terminal_and_never_loops_renewal() {
    let (url, worker) = mock_requests(
        vec![
            (
                200,
                json!({"access_token":"renewed-access","expires_in":3600}),
            ),
            (
                200,
                json!({"sub":"account-a","email":"fixture@example.invalid"}),
            ),
        ],
        |_, _| {},
    );
    let (service, path) = service(&url);
    let calls = AtomicUsize::new(0);
    let error = service
        .authenticated_read(|_, _| {
            calls.fetch_add(1, Ordering::SeqCst);
            Err(ServiceError {
                code: "http_unauthorized",
                message: "Unauthorized fixture".into(),
            })
        })
        .unwrap_err();
    assert_eq!(error.code, "http_unauthorized");
    assert_eq!(calls.load(Ordering::SeqCst), 2);
    worker.join().unwrap();
    std::fs::remove_dir_all(path).unwrap();
}

#[test]
fn delayed_library_result_cannot_publish_after_account_replacement() {
    let (entered_tx, entered_rx) = std::sync::mpsc::channel();
    let (release_tx, release_rx) = std::sync::mpsc::channel();
    let (url, worker) = mock_requests(
        vec![
            (200, json!({"requestStatus":{"serverId":"vpc-a"}})),
            (
                200,
                json!({"data":{"apps":{"items":[],"pageInfo":{"hasNextPage":false}}}}),
            ),
        ],
        move |index, _| {
            if index == 1 {
                entered_tx.send(()).unwrap();
                release_rx.recv_timeout(Duration::from_secs(5)).unwrap();
            }
        },
    );
    let (service, path) = service(&url);
    std::thread::scope(|threads| {
        let read = threads.spawn(|| service.library_catalog(&json!({}), &json!({})));
        entered_rx.recv_timeout(Duration::from_secs(5)).unwrap();
        {
            let mut state = service.state.lock().unwrap();
            state.session = Some(auth_fixture("account-b"));
            state.generation += 1;
        }
        release_tx.send(()).unwrap();
        assert_eq!(read.join().unwrap().unwrap_err().code, "stale_account");
    });
    worker.join().unwrap();
    std::fs::remove_dir_all(path).unwrap();
}

#[test]
fn region_overrides_require_current_provider_membership_and_preserve_saved_preferences() {
    let (url, worker) = mock_requests(
        vec![(
            200,
            json!({"requestStatus":{"serverId":"alliance-vpc"},"metaData":[
                {"key":"Alliance region","value":"https://alliance-region.nvidiagrid.net/"},
                {"key":"Untrusted","value":"https://outside.invalid/"}
            ]}),
        )],
        |_, request| assert!(request.starts_with("GET /v2/serverInfo ")),
    );
    let (service, path) = service(&url);
    let mut session = auth_fixture("account-a");
    session.provider.idp_id = "alliance".into();
    session.provider.streaming_service_url = "https://alliance.nvidiagrid.net/".into();
    service.state.lock().unwrap().providers = vec![session.provider.clone()];
    let settings = json!({"region":"https://nvidia-region.nvidiagrid.net/","regionProviderIdpId":DEFAULT_IDP_ID});
    let (params, effective) = service
        .scoped_session_route(&json!({}), &settings, &session)
        .unwrap();
    assert!(params["streamingBaseUrl"].is_null());
    assert_eq!(effective["region"], "");
    let (params, _) = service
        .scoped_session_route(
            &json!({"streamingBaseUrl":"https://nvidia-region.nvidiagrid.net/"}),
            &settings,
            &session,
        )
        .unwrap();
    assert!(params["streamingBaseUrl"].is_null());
    assert_eq!(settings["region"], "https://nvidia-region.nvidiagrid.net/");
    worker.join().unwrap();
    std::fs::remove_dir_all(path).unwrap();
}

#[test]
fn endpoint_validation_is_shared_and_rejects_credential_exfiltration_shapes() {
    for url in [
        "https://prod.cloudmatchbeta.nvidiagrid.net/",
        "https://provider.nvidiagrid.net/",
        "https://region.geforcenow.nvidiagrid.net/",
    ] {
        assert!(trusted_streaming_base(url).is_ok());
    }
    for url in [
        "http://provider.nvidiagrid.net/",
        "https://provider.nvidiagrid.net.evil.test/",
        "https://user@provider.nvidiagrid.net/",
        "https://provider.nvidiagrid.net:8443/",
        "https://127.0.0.1/",
        "https://partner.invalid/",
    ] {
        assert!(trusted_streaming_base(url).is_err(), "{url}");
    }
}

#[test]
fn preparation_uses_owned_context_and_rejects_old_generations_or_other_seats() {
    let (service, path) = service("http://127.0.0.1:1");
    let owner = auth_fixture("account-a");
    service.cloudmatch.seed_owned_session(json!({"sessionId":"seat-a","subSessionId":"sub-a","status":3,"rtspsEndpoints":["rtsps://owned.nvidiagrid.net:443"],"connectionInfo":[{"protocol":"RTSPS","host":"owned.nvidiagrid.net","port":443}]}));
    service.session_routing.lock().unwrap().active_owner = Some((owner, 7));
    let params = json!({"session":{"sessionId":"seat-a","status":3,"connectionInfo":[{"host":"evil.invalid"}]}});
    let calls = AtomicUsize::new(0);
    let result = service
        .prepare_owned_stream(&params, |params| {
            calls.fetch_add(1, Ordering::SeqCst);
            assert_eq!(
                params["session"]["connectionInfo"][0]["host"],
                "owned.nvidiagrid.net"
            );
            Ok(params.clone())
        })
        .unwrap();
    assert_eq!(result["session"]["sessionId"], "seat-a");
    assert_eq!(calls.load(Ordering::SeqCst), 1);
    assert_eq!(
        service
            .prepare_owned_stream(&json!({"session":{"sessionId":"other"}}), |_| panic!(
                "foreign seat prepared"
            ))
            .unwrap_err()
            .code,
        "session_owner_mismatch"
    );
    service.state.lock().unwrap().generation += 1;
    assert_eq!(
        service
            .prepare_owned_stream(&params, |_| panic!("stale seat prepared"))
            .unwrap_err()
            .code,
        "session_owner_mismatch"
    );
    std::fs::remove_dir_all(path).unwrap();
}
