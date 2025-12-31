use tracing::Span;

pub fn remote_trace(name: &str) -> Span {
    tracing::info_span!("remote", name = name)
}
