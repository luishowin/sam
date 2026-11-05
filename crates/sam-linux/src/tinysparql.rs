//! The handful of TinySPARQL calls SAM needs, bound by hand (§0 P4, SPIKE-B).
//!
//! Only prepared statements are exposed, so text reaches a query as a bound
//! parameter or not at all (F10).

use std::ffi::{CStr, CString, c_char, c_int, c_long};
use std::ptr::{self, NonNull};

use glib::translate::*;

#[repr(C)]
struct TrackerSparqlConnection {
    _private: [u8; 0],
}
#[repr(C)]
struct TrackerSparqlStatement {
    _private: [u8; 0],
}
#[repr(C)]
struct TrackerSparqlCursor {
    _private: [u8; 0],
}

type GError = glib::ffi::GError;
type GCancellable = gio::ffi::GCancellable;

unsafe extern "C" {
    fn tracker_sparql_connection_bus_new(
        service_name: *const c_char,
        object_path: *const c_char,
        dbus_connection: *mut gio::ffi::GDBusConnection,
        error: *mut *mut GError,
    ) -> *mut TrackerSparqlConnection;
    fn tracker_sparql_connection_query_statement(
        connection: *mut TrackerSparqlConnection,
        sparql: *const c_char,
        cancellable: *mut GCancellable,
        error: *mut *mut GError,
    ) -> *mut TrackerSparqlStatement;
    fn tracker_sparql_connection_close(connection: *mut TrackerSparqlConnection);
    fn tracker_sparql_statement_bind_string(
        stmt: *mut TrackerSparqlStatement,
        name: *const c_char,
        value: *const c_char,
    );
    fn tracker_sparql_statement_bind_int(
        stmt: *mut TrackerSparqlStatement,
        name: *const c_char,
        value: i64,
    );
    fn tracker_sparql_statement_bind_datetime(
        stmt: *mut TrackerSparqlStatement,
        name: *const c_char,
        value: *mut glib::ffi::GDateTime,
    );
    fn tracker_sparql_statement_clear_bindings(stmt: *mut TrackerSparqlStatement);
    fn tracker_sparql_statement_execute(
        stmt: *mut TrackerSparqlStatement,
        cancellable: *mut GCancellable,
        error: *mut *mut GError,
    ) -> *mut TrackerSparqlCursor;
    fn tracker_sparql_cursor_next(
        cursor: *mut TrackerSparqlCursor,
        cancellable: *mut GCancellable,
        error: *mut *mut GError,
    ) -> glib::ffi::gboolean;
    fn tracker_sparql_cursor_get_string(
        cursor: *mut TrackerSparqlCursor,
        column: c_int,
        length: *mut c_long,
    ) -> *const c_char;
    fn tracker_sparql_cursor_close(cursor: *mut TrackerSparqlCursor);
}

fn cstring(s: &str) -> CString {
    // Interior NULs cannot be passed to C; nothing SAM binds needs them.
    CString::new(s.replace('\0', "")).expect("NULs were removed")
}

/// Takes ownership of `err` if set.
unsafe fn take_error(err: *mut GError) -> Option<glib::Error> {
    (!err.is_null()).then(|| unsafe { from_glib_full(err) })
}

/// A connection to a SPARQL endpoint on the session bus.
pub struct Connection(NonNull<TrackerSparqlConnection>);

// TinySPARQL documents connections as usable from any thread.
unsafe impl Send for Connection {}
unsafe impl Sync for Connection {}

impl Connection {
    /// LocalSearch's endpoint.
    pub fn localsearch() -> Result<Self, glib::Error> {
        Self::bus("org.freedesktop.LocalSearch3")
    }

    /// Blocks while it talks to the bus.
    pub fn bus(service: &str) -> Result<Self, glib::Error> {
        let service = cstring(service);
        let mut err = ptr::null_mut();
        let raw = unsafe {
            tracker_sparql_connection_bus_new(
                service.as_ptr(),
                ptr::null(),
                ptr::null_mut(),
                &mut err,
            )
        };
        if let Some(e) = unsafe { take_error(err) } {
            return Err(e);
        }
        NonNull::new(raw)
            .map(Connection)
            .ok_or_else(|| glib::Error::new(gio::IOErrorEnum::Failed, "no connection and no error"))
    }

    /// Parses `sparql` once; bind and execute it as often as needed.
    pub fn statement(&self, sparql: &str) -> Result<Statement, glib::Error> {
        let sparql = cstring(sparql);
        let mut err = ptr::null_mut();
        let raw = unsafe {
            tracker_sparql_connection_query_statement(
                self.0.as_ptr(),
                sparql.as_ptr(),
                ptr::null_mut(),
                &mut err,
            )
        };
        if let Some(e) = unsafe { take_error(err) } {
            return Err(e);
        }
        NonNull::new(raw)
            .map(Statement)
            .ok_or_else(|| glib::Error::new(gio::IOErrorEnum::Failed, "no statement and no error"))
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        unsafe {
            tracker_sparql_connection_close(self.0.as_ptr());
            glib::gobject_ffi::g_object_unref(self.0.as_ptr().cast());
        }
    }
}

/// A prepared query. Bindings are state, so a statement is used from one
/// thread at a time (`Send`, not `Sync`).
pub struct Statement(NonNull<TrackerSparqlStatement>);

unsafe impl Send for Statement {}

impl Statement {
    /// Binds `~name`.
    pub fn bind_str(&mut self, name: &str, value: &str) {
        let (name, value) = (cstring(name), cstring(value));
        unsafe {
            tracker_sparql_statement_bind_string(self.0.as_ptr(), name.as_ptr(), value.as_ptr())
        }
    }

    pub fn bind_int(&mut self, name: &str, value: i64) {
        let name = cstring(name);
        unsafe { tracker_sparql_statement_bind_int(self.0.as_ptr(), name.as_ptr(), value) }
    }

    /// Binds `~name` to a UTC instant given in Unix seconds.
    pub fn bind_unix_time(&mut self, name: &str, unix: i64) -> Result<(), glib::BoolError> {
        let time = glib::DateTime::from_unix_utc(unix)?;
        let name = cstring(name);
        unsafe {
            tracker_sparql_statement_bind_datetime(
                self.0.as_ptr(),
                name.as_ptr(),
                time.to_glib_none().0,
            )
        };
        Ok(())
    }

    pub fn clear(&mut self) {
        unsafe { tracker_sparql_statement_clear_bindings(self.0.as_ptr()) }
    }

    /// Runs the query with the current bindings. Blocks.
    pub fn execute(&mut self) -> Result<Cursor, glib::Error> {
        let mut err = ptr::null_mut();
        let raw =
            unsafe { tracker_sparql_statement_execute(self.0.as_ptr(), ptr::null_mut(), &mut err) };
        if let Some(e) = unsafe { take_error(err) } {
            return Err(e);
        }
        NonNull::new(raw)
            .map(Cursor)
            .ok_or_else(|| glib::Error::new(gio::IOErrorEnum::Failed, "no cursor and no error"))
    }
}

impl Drop for Statement {
    fn drop(&mut self) {
        unsafe { glib::gobject_ffi::g_object_unref(self.0.as_ptr().cast()) }
    }
}

/// Rows of a result, read forward.
pub struct Cursor(NonNull<TrackerSparqlCursor>);

impl Cursor {
    /// Moves to the next row; `false` once past the last.
    pub fn next_row(&mut self) -> Result<bool, glib::Error> {
        let mut err = ptr::null_mut();
        let more =
            unsafe { tracker_sparql_cursor_next(self.0.as_ptr(), ptr::null_mut(), &mut err) };
        match unsafe { take_error(err) } {
            Some(e) => Err(e),
            None => Ok(more != glib::ffi::GFALSE),
        }
    }

    /// The current row's `column`, or `None` when it is unbound.
    pub fn string(&self, column: i32) -> Option<String> {
        let raw =
            unsafe { tracker_sparql_cursor_get_string(self.0.as_ptr(), column, ptr::null_mut()) };
        (!raw.is_null()).then(|| {
            unsafe { CStr::from_ptr(raw) }
                .to_string_lossy()
                .into_owned()
        })
    }
}

impl Drop for Cursor {
    fn drop(&mut self) {
        unsafe {
            tracker_sparql_cursor_close(self.0.as_ptr());
            glib::gobject_ffi::g_object_unref(self.0.as_ptr().cast());
        }
    }
}
