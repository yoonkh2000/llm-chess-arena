use crate::model::AppData;

pub const DB_NAME: &str = "llm-chess-arena";
pub const STORE_NAME: &str = "state";

pub fn export_json(data: &AppData) -> Result<String, serde_json::Error> {
    serde_json::to_string_pretty(data)
}

pub fn import_json(json: &str) -> Result<AppData, String> {
    let data: AppData = serde_json::from_str(json).map_err(|error| error.to_string())?;
    if data.schema_version > crate::model::SCHEMA_VERSION {
        return Err(format!(
            "지원하지 않는 백업 버전입니다: {}",
            data.schema_version
        ));
    }
    Ok(data)
}

#[cfg(target_arch = "wasm32")]
pub async fn load() -> Result<AppData, String> {
    use rexie::{ObjectStore, Rexie, TransactionMode};
    use wasm_bindgen::JsValue;

    let db = Rexie::builder(DB_NAME)
        .version(1)
        .add_object_store(ObjectStore::new(STORE_NAME))
        .build()
        .await
        .map_err(|error| error.to_string())?;
    let tx = db
        .transaction(&[STORE_NAME], TransactionMode::ReadOnly)
        .map_err(|error| error.to_string())?;
    let store = tx.store(STORE_NAME).map_err(|error| error.to_string())?;
    let value = store
        .get(JsValue::from_str("app"))
        .await
        .map_err(|error| error.to_string())?;
    match value {
        Some(value) => serde_wasm_bindgen::from_value(value).map_err(|error| error.to_string()),
        None => Ok(AppData::default()),
    }
}

#[cfg(target_arch = "wasm32")]
pub async fn save(data: &AppData) -> Result<(), String> {
    use rexie::{ObjectStore, Rexie, TransactionMode};
    use wasm_bindgen::JsValue;

    let db = Rexie::builder(DB_NAME)
        .version(1)
        .add_object_store(ObjectStore::new(STORE_NAME))
        .build()
        .await
        .map_err(|error| error.to_string())?;
    let tx = db
        .transaction(&[STORE_NAME], TransactionMode::ReadWrite)
        .map_err(|error| error.to_string())?;
    let store = tx.store(STORE_NAME).map_err(|error| error.to_string())?;
    let value = serde_wasm_bindgen::to_value(data).map_err(|error| error.to_string())?;
    let key = JsValue::from_str("app");
    store
        .put(&value, Some(&key))
        .await
        .map_err(|error| error.to_string())?;
    tx.done()
        .await
        .map(|_| ())
        .map_err(|error| error.to_string())
}
