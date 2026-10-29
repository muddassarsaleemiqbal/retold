//! Procfs field numbering begins after the final closing parenthesis of the command name.
use std::io;

const GROUP_FIELD: usize = 2;
const START_FIELD: usize = 19;

pub(super) fn parse(stat: &str) -> io::Result<(u32, u64)> {
    let (_, fields) = stat.rsplit_once(')').ok_or_else(invalid)?;
    let fields: Vec<_> = fields.split_whitespace().collect();
    Ok((number(&fields, GROUP_FIELD)?, number(&fields, START_FIELD)?))
}

fn number<T: std::str::FromStr>(fields: &[&str], index: usize) -> io::Result<T>
where
    T::Err: std::error::Error + Send + Sync + 'static,
{
    let field = fields.get(index).ok_or_else(invalid)?;
    field.parse().map_err(io::Error::other)
}

fn invalid() -> io::Error {
    io::Error::other("Native process stat is incomplete.")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_group_and_start_after_command_parentheses_and_spaces() {
        let stat = "123 (a ) tricky (command)) S 1 123 99 0 -1 0 0 0 0 0 0 0 0 0 0 0 1 0 7654321 0";
        assert_eq!(parse(stat).unwrap(), (123, 7_654_321));
        assert!(parse("123 malformed").is_err());
        assert!(parse("123 (short) S 1").is_err());
    }
}
