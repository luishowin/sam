fn main() {
    // TinySPARQL, linked by soname: Fedora ships the library without its
    // unversioned symlink, .pc or .gir unless -devel is installed (§0 F18).
    println!("cargo:rustc-link-lib=dylib:+verbatim=libtinysparql-3.0.so.0");
    // libcurl, the same way (§0 F23). SAM maps it already, through
    // libadwaita → libappstream.
    println!("cargo:rustc-link-lib=dylib:+verbatim=libcurl.so.4");
}
