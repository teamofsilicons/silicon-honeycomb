use super::*;
use std::sync::Mutex;
struct Bundles {
    record: Mutex<Option<Value>>,
    fail: AtomicBool,
    deny: AtomicBool,
    wrong_receipt: AtomicBool,
    receipts: Mutex<BTreeMap<String, Value>>,
}
#[async_trait]
impl Management for Bundles {
    async fn configure(&self, _: &Value, _: &str, _: Option<&str>) -> Result<Value> {
        unreachable!()
    }
    async fn lifecycle(&self, _: &Value, _: &str) -> Result<Value> {
        unreachable!()
    }
    async fn bundle(&self, id: &str) -> Result<Value> {
        self.record
            .lock()
            .unwrap()
            .clone()
            .filter(|r| r["bundle_id"] == id)
            .ok_or_else(Error::missing)
    }
    async fn bundle_inventory(&self, _: Option<uuid::Uuid>) -> Result<Value> {
        Ok(
            json!({"items":[{"resource_id":"tos>interface"},{"resource_id":"other>private"}],"next_after":null}),
        )
    }
    async fn configure_bundle(&self, id: &str, body: &Value, actor: &str) -> Result<Value> {
        assert_eq!(actor, "admin");
        if self.deny.load(Ordering::SeqCst) {
            return Err(Error::forbidden());
        }
        let op = body["operation_id"].as_str().unwrap();
        if let Some(receipt) = self.receipts.lock().unwrap().get(op).cloned() {
            return Ok(receipt);
        }
        let record = json!({"bundle_id":id,"org_id":"tos","id":"preserved-identity","app_name":body["app_name"],"app_logo":body["app_logo"],"app_ids":body["app_ids"],"configuration_revision":body["configuration_revision"],"iam_revision":body["expected_iam_revision"].as_i64().unwrap()+1,"deleted":false});
        let mut receipt = json!({"operation_id":op,"state":"accepted","iam_revision":record["iam_revision"],"effective_configuration":record});
        if self.wrong_receipt.load(Ordering::SeqCst) {
            receipt["effective_configuration"]["bundle_id"] = json!("tos>wrong");
            return Ok(receipt);
        }
        *self.record.lock().unwrap() = Some(record);
        self.receipts
            .lock()
            .unwrap()
            .insert(op.into(), receipt.clone());
        // IAM committed, but the first response was lost.
        if self.fail.swap(false, Ordering::SeqCst) {
            return Err(Error::unavailable("transport interrupted"));
        }
        Ok(receipt)
    }
}
async fn fixture() -> (State, Arc<Bundles>) {
    let (mut s, _) = setup(true).await;
    let manager = Arc::new(Bundles {
        record: Mutex::new(None),
        fail: AtomicBool::new(false),
        deny: AtomicBool::new(false),
        wrong_receipt: AtomicBool::new(false),
        receipts: Mutex::new(BTreeMap::new()),
    });
    s.management = manager.clone();
    (s, manager)
}
fn definition() -> Value {
    json!({"app_name":"Silicon Interface","app_ids":["tos>iam","tos>dm","tos>briefcase","tos>commit","tos>remind","tos>waveform","tos>browser","tos>starter"]})
}
const PATH: &str = "/api/v1/bundles/tos%3Einterface";
const KEY: &str = "bundle-create-request-1";
#[tokio::test]
async fn bundle_authority_and_member_validation_fail_before_management() {
    let (s, manager) = fixture().await;
    for (token, status) in [
        (None, StatusCode::UNAUTHORIZED),
        (Some("member"), StatusCode::FORBIDDEN),
        (Some("outsider"), StatusCode::FORBIDDEN),
    ] {
        assert_eq!(
            call(&s, "PUT", PATH, token, definition(), KEY, Some(0))
                .await
                .0,
            status
        );
    }
    for members in [
        json!([]),
        json!(["tos>iam", "tos>iam"]),
        json!(["other>app"]),
        json!(["tos>interface"]),
    ] {
        let mut body = definition();
        body["app_ids"] = members;
        assert_eq!(
            call(&s, "PUT", PATH, Some("admin"), body, KEY, Some(0))
                .await
                .0,
            StatusCode::BAD_REQUEST
        );
    }
    assert!(manager.receipts.lock().unwrap().is_empty());
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM operations")
        .fetch_one(&s.db)
        .await
        .unwrap();
    assert_eq!(count, 0);
    manager.deny.store(true, Ordering::SeqCst);
    assert_eq!(
        call(&s, "PUT", PATH, Some("admin"), definition(), KEY, Some(0))
            .await
            .0,
        StatusCode::FORBIDDEN,
        "IAM eligibility remains authoritative even for a Honeycomb admin"
    );
    assert!(manager.record.lock().unwrap().is_none());
}
#[tokio::test]
async fn bundle_retries_preserve_identity_and_do_not_apply_twice() {
    let (s, manager) = fixture().await;
    manager.fail.store(true, Ordering::SeqCst);
    assert_eq!(
        call(&s, "PUT", PATH, Some("admin"), definition(), KEY, Some(0))
            .await
            .0,
        StatusCode::SERVICE_UNAVAILABLE
    );
    let (status, receipt) = call(&s, "PUT", PATH, Some("admin"), definition(), KEY, Some(0)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(receipt["iam_revision"], 1);
    assert_eq!(
        receipt["effective_configuration"]["app_ids"]
            .as_array()
            .unwrap()
            .len(),
        8
    );
    assert_eq!(
        call(&s, "PUT", PATH, Some("admin"), definition(), KEY, Some(0))
            .await
            .1,
        receipt
    );
    assert_eq!(manager.receipts.lock().unwrap().len(), 1);
    let mut changed = definition();
    changed["app_name"] = json!("Updated");
    assert_eq!(
        call(
            &s,
            "PUT",
            PATH,
            Some("admin"),
            changed.clone(),
            KEY,
            Some(0)
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    assert_eq!(
        call(
            &s,
            "PUT",
            PATH,
            Some("admin"),
            changed.clone(),
            "bundle-update-request-1",
            Some(0)
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    let (status, updated) = call(
        &s,
        "PUT",
        PATH,
        Some("admin"),
        changed,
        "bundle-update-request-1",
        Some(1),
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(updated["iam_revision"], 2);
    assert_eq!(
        updated["effective_configuration"]["id"],
        "preserved-identity"
    );
    let (status, list) = call(
        &s,
        "GET",
        "/api/v1/bundles",
        Some("admin"),
        Value::Null,
        KEY,
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(list["items"].as_array().unwrap().len(), 1);
    let (_, list) = call(
        &s,
        "GET",
        "/api/v1/bundles",
        Some("member"),
        Value::Null,
        KEY,
        None,
    )
    .await;
    assert_eq!(list["items"], json!([]));
}
#[tokio::test]
async fn bundle_rejects_wrong_receipt_and_serializes_pending_writes() {
    let (s, manager) = fixture().await;
    manager.wrong_receipt.store(true, Ordering::SeqCst);
    assert_eq!(
        call(&s, "PUT", PATH, Some("admin"), definition(), KEY, Some(0))
            .await
            .0,
        StatusCode::SERVICE_UNAVAILABLE
    );
    assert_eq!(
        call(
            &s,
            "PUT",
            PATH,
            Some("admin"),
            definition(),
            "bundle-other-request",
            Some(0)
        )
        .await
        .0,
        StatusCode::CONFLICT
    );
    manager.wrong_receipt.store(false, Ordering::SeqCst);
    assert_eq!(
        call(&s, "PUT", PATH, Some("admin"), definition(), KEY, Some(0))
            .await
            .0,
        StatusCode::OK
    );
}
