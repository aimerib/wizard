use shell_words::ParseError;
extern crate shell_words;

pub(crate) trait IntoArgs {
    fn try_into_args(&self) -> Result<Vec<String>, ParseError>;
}

impl<S: std::ops::Deref<Target = str>> IntoArgs for S {
    fn try_into_args(&self) -> Result<Vec<String>, ParseError> {
        shell_words::split(self)
    }
}
