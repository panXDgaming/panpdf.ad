use std::cell::RefCell;

use super::*;

struct Seen {
    method: String,
    url: String,
    headers: Vec<String>,
    body: String,
}

thread_local! {
    static ANSWERS: RefCell<Vec<Result<crate::transport::HttpAnswer, TransportError>>> =
        const { RefCell::new(Vec::new()) };
    static SEEN: RefCell<Vec<Seen>> = const { RefCell::new(Vec::new()) };
}

fn fake(call: &HttpCall<'_>) -> Result<crate::transport::HttpAnswer, TransportError> {
    SEEN.with(|seen| {
        seen.borrow_mut().push(Seen {
            method: call.method.to_owned(),
            url: call.url.to_owned(),
            headers: call.headers.to_vec(),
            body: call
                .body
                .map(|body| String::from_utf8_lossy(body).into_owned())
                .unwrap_or_default(),
        });
    });
    ANSWERS.with(|answers| {
        let mut answers = answers.borrow_mut();
        if answers.is_empty() {
            Err(TransportError::Unreachable(
                "nothing to answer with".to_owned(),
            ))
        } else {
            answers.remove(0)
        }
    })
}

struct Restore;

impl Drop for Restore {
    fn drop(&mut self) {
        transport::try_with(None);
    }
}

fn answering(answers: Vec<Result<crate::transport::HttpAnswer, TransportError>>) -> Restore {
    transport::try_with(Some(fake));
    ANSWERS.with(|held| *held.borrow_mut() = answers);
    SEEN.with(|seen| seen.borrow_mut().clear());
    Restore
}

#[expect(
    clippy::unnecessary_wraps,
    reason = "it stands in a list with the answers that never come"
)]
fn said(
    status: u16,
    headers: &str,
    body: &str,
) -> Result<crate::transport::HttpAnswer, TransportError> {
    Ok(crate::transport::HttpAnswer {
        status,
        headers: headers.to_owned(),
        body: body.as_bytes().to_vec(),
    })
}

fn connection(provider: Provider, base: &str) -> Connection {
    Connection {
        provider,
        base_url: base.to_owned(),
        model: "a-model".into(),
        api_key: "a-key".into(),
        effort: Effort::Off,
    }
}

fn the_only_request() -> Seen {
    SEEN.with(|seen| {
        let mut seen = seen.borrow_mut();
        assert_eq!(seen.len(), 1, "one request was made");
        seen.remove(0)
    })
}

const CHAT_STREAM: &str = "\
data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"Reading \"}}]}

data: {\"choices\":[{\"index\":0,\"delta\":{\"content\":\"page one.\"}}]}

data: {\"choices\":[{\"index\":0,\"delta\":{},\"finish_reason\":\"stop\"}]}

data: [DONE]
";

const MESSAGES_STREAM: &str = "\
event: message_start
data: {\"type\":\"message_start\",\"message\":{\"id\":\"msg_1\",\"role\":\"assistant\",\"content\":[]}}

event: content_block_start
data: {\"type\":\"content_block_start\",\"index\":0,\"content_block\":{\"type\":\"text\",\"text\":\"\"}}

event: content_block_delta
data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"Reading \"}}

event: content_block_delta
data: {\"type\":\"content_block_delta\",\"index\":0,\"delta\":{\"type\":\"text_delta\",\"text\":\"page one.\"}}

event: content_block_stop
data: {\"type\":\"content_block_stop\",\"index\":0}

event: message_delta
data: {\"type\":\"message_delta\",\"delta\":{\"stop_reason\":\"end_turn\"}}

event: message_stop
data: {\"type\":\"message_stop\"}
";

const RESPONSES_STREAM: &str = "\
event: response.output_text.delta
data: {\"type\":\"response.output_text.delta\",\"delta\":\"Reading \"}

event: response.output_text.delta
data: {\"type\":\"response.output_text.delta\",\"delta\":\"page one.\"}

event: response.output_item.done
data: {\"type\":\"response.output_item.done\",\"item\":{\"type\":\"message\",\"content\":[{\"type\":\"output_text\",\"text\":\"Reading page one.\"}]}}

event: response.completed
data: {\"type\":\"response.completed\",\"response\":{\"status\":\"completed\"}}
";

#[test]
fn a_request_goes_through_the_registered_transport_with_its_address_headers_and_words() {
    let _back = answering(vec![said(
        200,
        "Content-Type: application/json\r\n",
        r#"{"choices":[{"finish_reason":"stop","message":{"role":"assistant","content":"fine"}}]}"#,
    )]);
    let reply = connection(Provider::Custom, "https://example.test/v1")
        .converse(
            &[Turn::person("hello there")],
            None,
            &AtomicBool::new(false),
        )
        .unwrap();
    assert_eq!(reply.text, "fine");
    let seen = the_only_request();
    assert_eq!(seen.method, "POST");
    assert_eq!(seen.url, "https://example.test/v1/chat/completions");
    assert!(
        seen.headers
            .contains(&"Authorization: Bearer a-key".to_owned()),
        "{:?}",
        seen.headers
    );
    assert!(
        seen.headers
            .contains(&"Content-Type: application/json".to_owned())
    );
    assert!(seen.body.contains("hello there"), "{}", seen.body);
    assert!(!seen.body.contains("\"stream\""), "{}", seen.body);
}

#[test]
fn a_list_of_models_goes_through_the_registered_transport_too() {
    let _back = answering(vec![said(
        200,
        "",
        r#"{"data":[{"id":"one"},{"id":"two"}]}"#,
    )]);
    let models = connection(Provider::Custom, "https://example.test/v1")
        .models(&AtomicBool::new(false))
        .unwrap();
    assert_eq!(
        models
            .iter()
            .map(|model| model.id.as_str())
            .collect::<Vec<_>>(),
        ["one", "two"]
    );
    let seen = the_only_request();
    assert_eq!(
        (seen.method.as_str(), seen.url.as_str()),
        ("GET", "https://example.test/v1/models")
    );
    assert!(seen.body.is_empty());
}

#[test]
fn a_streamed_answer_that_arrives_whole_is_read_as_it_would_have_arrived_in_pieces() {
    for (provider, base, stream) in [
        (Provider::Custom, "https://example.test/v1", CHAT_STREAM),
        (
            Provider::Anthropic,
            "https://example.test/v1",
            MESSAGES_STREAM,
        ),
        (
            Provider::OpenAi,
            "https://example.test/v1",
            RESPONSES_STREAM,
        ),
    ] {
        let _back = answering(vec![said(
            200,
            "Content-Type: text/event-stream\r\n",
            stream,
        )]);
        let mut partials: Vec<String> = Vec::new();
        let reply = connection(provider, base)
            .converse_streaming(
                &[Turn::person("read it")],
                None,
                &[],
                None,
                &AtomicBool::new(false),
                &mut |progress| partials.push(progress.said.to_owned()),
            )
            .unwrap();
        assert_eq!(reply.text, "Reading page one.", "{provider:?}");
        assert_eq!(
            partials.last().map(String::as_str),
            Some("Reading page one."),
            "{provider:?}: the window is told the answer"
        );
        let seen = the_only_request();
        assert!(
            seen.body.contains("\"stream\":true"),
            "{provider:?}: {}",
            seen.body
        );
    }
}

#[test]
fn a_streamed_request_whose_server_answers_in_one_json_body_is_read_all_the_same() {
    for (provider, whole) in [
        (
            Provider::Custom,
            r#"{"choices":[{"finish_reason":"stop","message":{"role":"assistant","content":"Reading page one."}}]}"#,
        ),
        (
            Provider::Anthropic,
            r#"{"content":[{"type":"text","text":"Reading page one."}],"stop_reason":"end_turn"}"#,
        ),
        (
            Provider::OpenAi,
            r#"{"status":"completed","output":[{"type":"message","content":[{"type":"output_text","text":"Reading page one."}]}]}"#,
        ),
    ] {
        let _back = answering(vec![said(200, "", whole)]);
        let reply = connection(provider, "https://example.test/v1")
            .converse_streaming(
                &[Turn::person("read it")],
                None,
                &[],
                None,
                &AtomicBool::new(false),
                &mut |_| {},
            )
            .unwrap();
        assert_eq!(reply.text, "Reading page one.", "{provider:?}");
    }
}

#[test]
fn a_refusal_keeps_the_words_and_the_wait_the_service_asked_for() {
    let _back = answering(vec![said(
        429,
        "Content-Type: application/json\r\nRetry-After: 7\r\n",
        r#"{"error":{"message":"Rate limit reached for this model"}}"#,
    )]);
    let job = connection(Provider::Custom, "https://example.test/v1")
        .job("POST", "/chat/completions", Some("{}"), Pace::whole())
        .unwrap();
    let failure = run_job(&job, &AtomicBool::new(false), &mut |_| false).unwrap_err();
    assert!(
        matches!(&failure.error, ConnectError::Http(said) if said.contains("Rate limit reached")),
        "{failure:?}"
    );
    assert!(failure.transient);
    assert_eq!(failure.wait, Some(Duration::from_secs(7)));
}

#[test]
fn a_service_that_cannot_be_reached_is_tried_again_unless_it_is_on_this_phone() {
    let _back = answering(vec![Err(TransportError::Unreachable(
        "no route to host".to_owned(),
    ))]);
    let job = connection(Provider::Custom, "https://example.test/v1")
        .job("GET", "/models", None, Pace::listing())
        .unwrap();
    let failure = run_job(&job, &AtomicBool::new(false), &mut |_| false).unwrap_err();
    assert!(
        matches!(&failure.error, ConnectError::Curl(said) if said == "no route to host"),
        "{failure:?}"
    );
    assert!(failure.transient);

    let _back = answering(vec![Err(TransportError::Unreachable("refused".to_owned()))]);
    let job = connection(Provider::Ollama, "http://127.0.0.1:11434/v1")
        .job("GET", "/models", None, Pace::listing())
        .unwrap();
    let failure = run_job(&job, &AtomicBool::new(false), &mut |_| false).unwrap_err();
    assert!(
        !failure.transient,
        "a server on this device is not there or it is: {failure:?}"
    );
}

#[test]
fn a_request_that_waited_too_long_says_how_long_and_is_tried_once_more() {
    let _back = answering(vec![Err(TransportError::TimedOut)]);
    let job = connection(Provider::Custom, "https://example.test/v1")
        .job("POST", "/chat/completions", Some("{}"), Pace::listing())
        .unwrap();
    let failure = run_job(&job, &AtomicBool::new(false), &mut |_| false).unwrap_err();
    assert!(
        matches!(&failure.error, ConnectError::Curl(said) if said.contains("30 seconds")),
        "{failure:?}"
    );
    assert!(failure.transient);
    assert_eq!(failure.tries, 1);
}

#[test]
fn a_request_stopped_before_it_is_sent_is_not_sent_and_one_stopped_on_the_way_says_so() {
    let _back = answering(vec![said(200, "", "{}")]);
    let job = connection(Provider::Custom, "https://example.test/v1")
        .job("GET", "/models", None, Pace::listing())
        .unwrap();
    let failure = run_job(&job, &AtomicBool::new(true), &mut |_| false).unwrap_err();
    assert_eq!(failure.error, ConnectError::Cancelled);
    assert!(
        SEEN.with(|seen| seen.borrow().is_empty()),
        "nothing was sent"
    );

    let _back = answering(vec![Err(TransportError::Cancelled)]);
    let failure = run_job(&job, &AtomicBool::new(false), &mut |_| false).unwrap_err();
    assert_eq!(failure.error, ConnectError::Cancelled);
    assert!(!failure.transient);
}

#[test]
fn an_error_page_is_refused_by_its_status_and_not_parsed_as_an_answer() {
    let _back = answering(vec![said(502, "", "<html>bad gateway</html>")]);
    let job = connection(Provider::Custom, "https://example.test/v1")
        .job("POST", "/chat/completions", Some("{}"), Pace::whole())
        .unwrap();
    let failure = run_job(&job, &AtomicBool::new(false), &mut |_| false).unwrap_err();
    assert!(
        matches!(failure.error, ConnectError::Http(_)),
        "{failure:?}"
    );
    assert!(failure.transient, "a bad gateway is worth another try");
}

#[test]
fn an_answer_that_is_not_text_or_is_too_large_is_refused() {
    let _back = answering(vec![Ok(crate::transport::HttpAnswer {
        status: 200,
        headers: String::new(),
        body: vec![0xff, 0xfe],
    })]);
    let job = connection(Provider::Custom, "https://example.test/v1")
        .job("GET", "/models", None, Pace::listing())
        .unwrap();
    let failure = run_job(&job, &AtomicBool::new(false), &mut |_| false).unwrap_err();
    assert!(
        matches!(failure.error, ConnectError::Protocol(_)),
        "{failure:?}"
    );

    let _back = answering(vec![Ok(crate::transport::HttpAnswer {
        status: 200,
        headers: String::new(),
        body: vec![b' '; MAX_RESPONSE + 1],
    })]);
    let failure = run_job(&job, &AtomicBool::new(false), &mut |_| false).unwrap_err();
    assert_eq!(failure.error, ConnectError::ResponseTooLarge);
}

#[test]
fn a_request_with_no_registered_transport_still_goes_to_curl() {
    transport::try_with(None);
    let job = connection(Provider::Custom, "https://example.test/v1")
        .job("GET", "/models", None, Pace::listing())
        .unwrap();
    assert!(job.call.is_none());
    assert!(
        job.config
            .contains("url = \"https://example.test/v1/models\"")
    );
}

#[test]
fn the_emulators_alias_for_its_host_is_a_local_address_only_on_a_phone() {
    assert_eq!(
        endpoint("http://10.0.2.2:11434/v1", "/models", Provider::Ollama).is_ok(),
        cfg!(target_os = "android")
    );
    assert!(endpoint("http://10.0.2.3:11434/v1", "/models", Provider::Ollama).is_err());
}
