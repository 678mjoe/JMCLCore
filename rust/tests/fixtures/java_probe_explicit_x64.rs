fn main() {
    for key in ["JAVA_TOOL_OPTIONS", "_JAVA_OPTIONS", "JDK_JAVA_OPTIONS"] {
        if std::env::var_os(key).is_some() {
            std::process::exit(3)
        }
    }
    eprintln!(
        "java.specification.version = 17\nos.arch = amd64\nsun.arch.data.model = 64\njava.vendor = Fixture\njava.home = /fixture/java"
    );
}
