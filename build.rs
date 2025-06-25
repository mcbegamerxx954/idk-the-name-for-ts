fn main() {
    cc::Build::new()
        .cpp(true)
        .file("src/mbl/string.cpp")
        .compile("stringstub");
}
