use std::env;



use pyo3::prelude::*;
use pyo3::types::{PyDict, PyList};
use thiserror::Error;

#[derive(Clone, Debug, Default)]
pub struct SaveEntry {
    pub directory: String,
    pub size: i64,
    pub modified: i64,
    pub description: String,
    pub protection: String,
}

#[derive(Debug, Error)]
pub enum BridgeError {
    #[error("{0}")]
    Python(String),
    #[error("{0}")]
    System(String),
}

impl From<std::io::Error> for BridgeError {
    fn from(value: std::io::Error) -> Self {
        Self::System(value.to_string())
    }
}

fn ensure_sys_path(py: Python<'_>) -> Result<(), BridgeError> {
    let sys = py
        .import("sys")
        .map_err(|e| BridgeError::Python(py_err_to_string(py, e)))?;
    let path = sys
        .getattr("path")
        .map_err(|e| BridgeError::Python(py_err_to_string(py, e)))?
        // pyo3 0.28: .downcast_into() → .cast_into()
        .cast_into::<PyList>()
        .map_err(|e| BridgeError::Python(py_err_to_string(py, e.into())))?;

    let cwd = env::current_dir()?.to_string_lossy().to_string();
    let found = path
        .iter()
        .any(|item| item.extract::<String>().map(|s| s == cwd).unwrap_or(false));
    if !found {
        path.insert(0, cwd)
            .map_err(|e| BridgeError::Python(py_err_to_string(py, e)))?;
    }
    Ok(())
}

fn api_module(py: Python<'_>) -> Result<Bound<'_, PyModule>, BridgeError> {
    ensure_sys_path(py)?;
    py.import("python_core.api")
        .map_err(|e| BridgeError::Python(py_err_to_string(py, e)))
}

fn py_err_to_string(py: Python<'_>, err: PyErr) -> String {
    match err.value(py).str() {
        Ok(s) => s.to_string_lossy().into_owned(),
        Err(_) => err.to_string(),
    }
}

/// Extract a typed value from a dict by key.
/// Inlined to avoid the HRTB `for<'a> pyo3::PyErr: From<...>` limitation
/// in pyo3 0.28's two-parameter FromPyObject trait.
macro_rules! dict_get {
    ($py:expr, $dict:expr, $key:expr, $ty:ty) => {{
        let item = $dict
            .get_item($key)
            .map_err(|e| BridgeError::Python(py_err_to_string($py, e)))?
            .ok_or_else(|| {
                BridgeError::Python(format!("missing key: {}", $key))
            })?;
        item.extract::<$ty>()
            .map_err(|e| BridgeError::Python(py_err_to_string($py, e)))?
    }};
}

pub fn list_saves(card_path: &str) -> Result<Vec<SaveEntry>, BridgeError> {
    // pyo3 0.28: Python::with_gil → Python::attach
    Python::attach(|py| {
        let api = api_module(py)?;
        let rows_obj = api
            .call_method1("list_saves", (card_path,))
            .map_err(|e| BridgeError::Python(py_err_to_string(py, e)))?;
        // pyo3 0.28: .downcast() → .cast()
        let rows = rows_obj
            .cast::<PyList>()
            .map_err(|e| BridgeError::Python(py_err_to_string(py, e.into())))?;

        let mut out = Vec::with_capacity(rows.len());
        for row in rows.iter() {
            let dict = row
                .cast::<PyDict>()
                .map_err(|e| BridgeError::Python(py_err_to_string(py, e.into())))?;
            let item = SaveEntry {
                directory:   dict_get!(py, dict, "directory",   String),
                size:        dict_get!(py, dict, "size",        i64),
                modified:    dict_get!(py, dict, "modified",    i64),
                description: dict_get!(py, dict, "description", String),
                protection:  dict_get!(py, dict, "protection",  String),
            };
            out.push(item);
        }
        Ok(out)
    })
}

pub fn get_free_space(card_path: &str) -> Result<i64, BridgeError> {
    Python::attach(|py| {
        let api = api_module(py)?;
        let value = api
            .call_method1("get_free_space", (card_path,))
            .map_err(|e| BridgeError::Python(py_err_to_string(py, e)))?;
        value
            .extract::<i64>()
            .map_err(|e| BridgeError::Python(py_err_to_string(py, e)))
    })
}

pub fn import_save(
    card_path: &str,
    save_path: &str,
    dest_dir: Option<&str>,
    ignore_existing: bool,
) -> Result<(), BridgeError> {
    Python::attach(|py| {
        let api = api_module(py)?;
        api.call_method1(
            "import_save",
            (card_path, save_path, dest_dir, ignore_existing),
        )
        .map_err(|e| BridgeError::Python(py_err_to_string(py, e)))?;
        Ok(())
    })
}

pub fn export_save(
    card_path: &str,
    dirname: &str,
    output_path: Option<&str>,
    fmt: &str,
) -> Result<String, BridgeError> {
    Python::attach(|py| {
        let api = api_module(py)?;
        let out = api
            .call_method1("export_save", (card_path, dirname, output_path, fmt))
            .map_err(|e| BridgeError::Python(py_err_to_string(py, e)))?;
        out.extract::<String>()
            .map_err(|e| BridgeError::Python(py_err_to_string(py, e)))
    })
}

pub fn delete_save(card_path: &str, dirname: &str) -> Result<(), BridgeError> {
    Python::attach(|py| {
        let api = api_module(py)?;
        api.call_method1("delete_save", (card_path, dirname))
            .map_err(|e| BridgeError::Python(py_err_to_string(py, e)))?;
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;
    use std::process::Command;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::*;

    fn temp_card_path() -> PathBuf {
        let mut path = std::env::temp_dir();
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("clock")
            .as_millis();
        path.push(format!("mymc_test_{stamp}.ps2"));
        path
    }

    #[test]
    fn bridge_lists_empty_card() {
        let card = temp_card_path();
        let status = Command::new("python3")
            .args(["mymc.py", card.to_string_lossy().as_ref(), "format"])
            .status()
            .expect("run mymc format");
        assert!(status.success());

        let list = list_saves(card.to_string_lossy().as_ref()).expect("list saves");
        assert!(list.is_empty());
    }
}
