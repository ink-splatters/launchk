use crate::{objects, xpc_retain};
use crate::{
    xpc_dictionary_apply, xpc_dictionary_create, xpc_dictionary_set_value, xpc_get_type,
    xpc_object_t, xpc_type_t,
};
use block::ConcreteBlock;
use std::cell::RefCell;
use std::collections::HashMap;
use std::convert::TryFrom;
use std::error::Error;
use std::ffi::{CStr, CString};
use std::fmt::{Display, Formatter};

use crate::objects::xpc_error::XPCError;
use crate::objects::xpc_error::XPCError::DictionaryError;
use crate::objects::xpc_object::XPCObject;
use std::os::raw::c_char;
use std::ptr::{null, null_mut};
use std::rc::Rc;

pub struct XPCDictionary(pub HashMap<String, XPCObject>);

impl XPCDictionary {
    /// Reify xpc_object_t dictionary as a Rust HashMap
    pub fn new(object: &XPCObject) -> Result<XPCDictionary, XPCError> {
        let XPCObject(_, object_type) = *object;

        if object_type != *objects::xpc_type::Dictionary {
            return Err(DictionaryError(
                "Only XPC_TYPE_DICTIONARY allowed".to_string(),
            ));
        }

        let map: Rc<RefCell<HashMap<String, XPCObject>>> = Rc::new(RefCell::new(HashMap::new()));
        let map_rc_clone = map.clone();

        let block = ConcreteBlock::new(move |key: *const c_char, value: xpc_object_t| {
            // Prevent xpc_release() collection on block exit
            unsafe { xpc_retain(value) };

            let str_key = unsafe { CStr::from_ptr(key).to_string_lossy().to_string() };
            map_rc_clone.borrow_mut().insert(str_key, value.into());
        });
        let block = block.copy();

        let ok = unsafe { xpc_dictionary_apply(object.as_ptr(), &*block as *const _ as *mut _) };

        // Explicitly drop the block so map is the only live reference
        // so we can collect it below
        std::mem::drop(block);

        if ok {
            match Rc::try_unwrap(map) {
                Ok(cell) => Ok(XPCDictionary(cell.into_inner())),
                Err(_) => Err(DictionaryError("Unable to unwrap Rc".to_string())),
            }
        } else {
            Err(DictionaryError("xpc_dictionary_apply failed".to_string()))
        }
    }
}

impl From<HashMap<String, XPCObject>> for XPCDictionary {
    fn from(dict: HashMap<String, XPCObject>) -> XPCDictionary {
        XPCDictionary(dict)
    }
}

impl TryFrom<&XPCObject> for XPCDictionary {
    type Error = XPCError;

    fn try_from(value: &XPCObject) -> Result<XPCDictionary, XPCError> {
        XPCDictionary::new(value)
    }
}

impl TryFrom<XPCObject> for XPCDictionary {
    type Error = XPCError;

    fn try_from(value: XPCObject) -> Result<XPCDictionary, XPCError> {
        XPCDictionary::new(&value)
    }
}

impl TryFrom<xpc_object_t> for XPCDictionary {
    type Error = XPCError;

    /// Creates a XPC dictionary from an xpc_object_t pointer. Errors are generally
    /// related to passing in objects other than XPC_TYPE_DICTIONARY
    fn try_from(value: xpc_object_t) -> Result<XPCDictionary, XPCError> {
        let obj: XPCObject = value.into();
        XPCDictionary::new(&obj)
    }
}

impl<S> From<HashMap<S, XPCObject>> for XPCObject
where
    S: Into<String>,
{
    /// Creates a XPC dictionary
    ///
    /// Values must be XPCObject newtype but can encapsulate any
    /// valid xpc_object_t
    fn from(message: HashMap<S, XPCObject>) -> Self {
        let dict = unsafe { xpc_dictionary_create(null(), null_mut(), 0) };

        for (k, v) in message {
            unsafe {
                let as_str: String = k.into();
                let cstr = CString::new(as_str);
                xpc_dictionary_set_value(dict, cstr.unwrap().as_ptr(), v.as_ptr());
            }
        }

        dict.into()
    }
}

impl From<&XPCDictionary> for XPCObject {
    fn from(XPCDictionary(map): &XPCDictionary) -> Self {
        map.clone().into()
    }
}

#[cfg(test)]
mod tests {
    use crate::object::xpc_dictionary::XPCDictionary;
    use crate::object::xpc_object::XPCObject;
    use crate::object::xpc_value::TryXPCValue;
    use crate::objects::xpc_dictionary::XPCDictionary;
    use crate::objects::xpc_object::XPCObject;
    use crate::traits::xpc_value::TryXPCValue;
    use crate::{xpc_dictionary_create, xpc_dictionary_get_string, xpc_dictionary_set_int64};
    use std::collections::HashMap;
    use std::convert::TryInto;
    use std::ffi::{CStr, CString};
    use std::ptr::{null, null_mut};

    #[test]
    fn raw_to_hashmap() {
        let raw_dict = unsafe { xpc_dictionary_create(null(), null_mut(), 0) };
        let key = CString::new("test").unwrap();
        let value: i64 = 42;

        unsafe { xpc_dictionary_set_int64(raw_dict, key.as_ptr(), value) };

        let XPCDictionary(map) = raw_dict.try_into().unwrap();
        if let Some(xpc_object) = map.get("test") {
            assert_eq!(value, xpc_object.xpc_value().unwrap());
        } else {
            panic!("Unable to get value from map");
        }
    }

    #[test]
    fn hashmap_to_raw() {
        let mut hm: HashMap<&str, XPCObject> = HashMap::new();
        let value = "foo";
        hm.insert("test", XPCObject::from(value));

        let xpc_object = XPCObject::from(hm);
        let cstr = unsafe {
            CStr::from_ptr(xpc_dictionary_get_string(
                xpc_object.as_ptr(),
                CString::new("test").unwrap().as_ptr(),
            ))
        };

        assert_eq!(cstr.to_str().unwrap(), value);
    }
}
