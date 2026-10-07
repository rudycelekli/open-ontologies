mod common;

use serde_json::{Value, json};
use std::process::Stdio;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};

struct Mcp {
    child: tokio::process::Child,
    stdin: tokio::process::ChildStdin,
    stdout: tokio::io::Lines<BufReader<tokio::process::ChildStdout>>,
    next_id: u64,
}

impl Mcp {
    async fn start(directory: &std::path::Path) -> Self {
        let (child, stdin, stdout) = {
            let _gate = common::exec_gate();
            let config = directory.join("config.toml");
            std::fs::write(
                &config,
                format!(
                    "[cache]\ndir = {}\n[embeddings]\nmodel_path = {}\ntokenizer_path = {}\n",
                    json!(directory.join("cache").to_string_lossy()),
                    json!(directory.join("absent.onnx").to_string_lossy()),
                    json!(directory.join("absent-tokenizer.json").to_string_lossy()),
                ),
            )
            .unwrap();
            let mut child = tokio::process::Command::new(env!("CARGO_BIN_EXE_open-ontologies"))
                .args(["--no-connect", "--data-dir"])
                .arg(directory)
                .args(["serve", "--config"])
                .arg(config)
                .stdin(Stdio::piped())
                .stdout(Stdio::piped())
                .stderr(Stdio::inherit())
                .kill_on_drop(true)
                .spawn()
                .unwrap();
            let stdin = child.stdin.take().unwrap();
            let stdout = BufReader::new(child.stdout.take().unwrap()).lines();
            (child, stdin, stdout)
        };
        let mut session = Self {
            child,
            stdin,
            stdout,
            next_id: 1,
        };
        session.request("initialize", json!({"protocolVersion":"2025-03-26", "capabilities":{}, "clientInfo":{"name":"unicode-rag-test", "version":"1"}})).await;
        session
            .write(json!({"jsonrpc":"2.0", "method":"notifications/initialized"}))
            .await;
        session
    }

    async fn write(&mut self, message: Value) {
        self.stdin
            .write_all(message.to_string().as_bytes())
            .await
            .unwrap();
        self.stdin.write_all(b"\n").await.unwrap();
        self.stdin.flush().await.unwrap();
    }

    async fn request(&mut self, method: &str, params: Value) -> Value {
        let id = self.next_id;
        self.next_id += 1;
        self.write(json!({"jsonrpc":"2.0", "id":id, "method":method, "params":params}))
            .await;
        loop {
            let line =
                tokio::time::timeout(std::time::Duration::from_secs(20), self.stdout.next_line())
                    .await
                    .expect("MCP response timeout")
                    .unwrap()
                    .expect("MCP output closed");
            let response: Value = serde_json::from_str(&line).unwrap();
            if response["id"] == id {
                assert!(response.get("error").is_none(), "{response}");
                return response["result"].clone();
            }
        }
    }

    async fn call(&mut self, name: &str, arguments: Value) -> Value {
        let result = self
            .request("tools/call", json!({"name":name, "arguments":arguments}))
            .await;
        let value: Value =
            serde_json::from_str(result["content"][0]["text"].as_str().unwrap()).unwrap();
        assert!(value.get("error").is_none(), "{value}");
        value
    }
}

use open_ontologies::eval_rag::{RagQa, evaluate, faithfulness, rouge_1_f1, token_jaccard};

struct Case {
    gold: &'static str,
    generated: &'static str,
    expected: f64,
}

fn cases() -> Vec<Case> {
    vec![
        Case {
            gold: "ТОВАР",
            generated: "товар",
            expected: 1.0,
        },
        Case {
            gold: "ÉTÉ",
            generated: "été",
            expected: 1.0,
        },
        Case {
            gold: "PRODUCT",
            generated: "product",
            expected: 1.0,
        },
        Case {
            gold: "PRODUCT, ORDER!",
            generated: "product order",
            expected: 1.0,
        },
        Case {
            gold: "注文商品",
            generated: "注文商品",
            expected: 1.0,
        },
        Case {
            gold: "ЗАКАЗ",
            generated: "товар",
            expected: 0.0,
        },
        Case {
            gold: "été",
            generated: "ete",
            expected: 0.0,
        },
        Case {
            gold: "ΟΣ",
            generated: "ος",
            expected: 1.0,
        },
    ]
}

fn qa(case: &Case) -> RagQa {
    RagQa {
        question_id: "ordinary-product".into(),
        gold_iri: "https://example.org/Product".into(),
        retrieved: vec!["https://example.org/Product".into()],
        generated_answer: Some(case.generated.into()),
        gold_answer: Some(case.gold.into()),
        retrieved_text: Some(case.gold.into()),
    }
}

fn assert_report(report: &Value, expected: f64) {
    for field in [
        "mean_faithfulness",
        "mean_answer_jaccard",
        "mean_answer_rouge1",
    ] {
        assert_eq!(report[field], expected);
    }
    for field in [
        "exact_match_at_1",
        "hit_at_3",
        "hit_at_5",
        "hit_at_10",
        "mrr",
    ] {
        assert_eq!(report[field], 1.0);
    }
    assert_eq!(report["total"], 1);
    assert_eq!(report["per_question_rank"], json!([1]));
}

#[test]
fn unicode_case_matches_use_the_same_tokens_in_all_three_answer_metrics() {
    for case in cases() {
        assert_eq!(faithfulness(case.generated, case.gold), case.expected);
        assert_eq!(token_jaccard(case.gold, case.generated), case.expected);
        assert_eq!(rouge_1_f1(case.gold, case.generated), case.expected);
        let report = serde_json::to_value(evaluate(&[qa(&case)])).unwrap();
        assert_report(&report, case.expected);
    }
}

#[tokio::test]
async fn public_mcp_rag_scores_unicode_case_without_changing_retrieval_metrics() {
    let tmp = tempfile::tempdir().unwrap();
    let mut mcp = Mcp::start(tmp.path()).await;
    for case in cases() {
        let report = mcp
            .call(
                "eval_rag",
                json!({"qa_json":serde_json::to_string(&[qa(&case)]).unwrap()}),
            )
            .await;
        assert_report(&report, case.expected);
    }
    mcp.child.kill().await.unwrap();
}
