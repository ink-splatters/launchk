use std::sync::Mutex;
use std::collections::HashMap;
use std::time::{SystemTime, Duration};
use std::convert::TryInto;

use crate::launchd::query::{LimitLoadToSessionType, find_in_all};
use crate::launchd::plist::LaunchdPlist;
use xpc_sys::traits::xpc_value::TryXPCValue;

const ENTRY_INFO_QUERY_TTL: Duration = Duration::from_secs(15);

lazy_static! {
    pub static ref ENTRY_STATUS_CACHE: Mutex<HashMap<String, LaunchdEntryStatus>> =
        Mutex::new(HashMap::new());
}

#[derive(Debug, Clone, Eq, PartialEq)]
pub struct LaunchdEntryStatus {
    pub plist: Option<LaunchdPlist>,
    pub limit_load_to_session_type: LimitLoadToSessionType,
    // So, there is a pid_t, but it's i32, and the XPC response has an i64?
    pub pid: i64,
    tick: SystemTime,
}

impl Default for LaunchdEntryStatus {
    fn default() -> Self {
        LaunchdEntryStatus {
            limit_load_to_session_type: LimitLoadToSessionType::Unknown,
            plist: None,
            pid: 0,
            tick: SystemTime::now(),
        }
    }
}

/// Get entry info for label
pub fn get_entry_status<S: Into<String>>(label: S) -> LaunchdEntryStatus {
    let label_string = label.into();
    let mut cache = ENTRY_STATUS_CACHE.try_lock().unwrap();

    if cache.contains_key(label_string.as_str()) {
        let item = cache.get(label_string.as_str()).unwrap().clone();

        if item.tick.elapsed().unwrap() > ENTRY_INFO_QUERY_TTL {
            cache.remove(label_string.as_str());
            drop(cache);
            return get_entry_status(label_string);
        }

        return item;
    }

    let meta = build_entry_status(&label_string);
    cache.insert(label_string, meta.clone());
    meta
}

fn build_entry_status<S: Into<String>>(label: S) -> LaunchdEntryStatus {
    let label_string = label.into();
    let response = find_in_all(label_string.clone());

    let pid: i64 = response
        .as_ref()
        .map_err(|e| e.clone())
        .and_then(|r| r.get(&["service", "PID"]))
        .and_then(|o| o.xpc_value())
        .unwrap_or(0);

    let limit_load_to_session_type = response
        .as_ref()
        .map_err(|e| e.clone())
        .and_then(|r| r.get(&["service", "LimitLoadToSessionType"]))
        .and_then(|o| o.try_into())
        .unwrap_or(LimitLoadToSessionType::Unknown);

    let entry_config = crate::launchd::plist::for_label(label_string.clone());

    LaunchdEntryStatus {
        limit_load_to_session_type,
        plist: entry_config,
        pid,
        tick: SystemTime::now(),
    }
}