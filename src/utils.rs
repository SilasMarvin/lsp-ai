use lsp_server::ResponseError;

pub trait ToResponseError {
    fn to_response_error(&self, code: i32) -> ResponseError;
}

impl ToResponseError for anyhow::Error {
    fn to_response_error(&self, code: i32) -> ResponseError {
        ResponseError {
            code: -32603,
            message: self.to_string(),
            data: None,
        }
    }
}
