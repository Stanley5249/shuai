use super::{Error, nonnegative};

/// Reconstructs into reusable scratch so validation can precede all mutation.
pub fn patch(old: &[i64], diff: &[i64], output: &mut Vec<i64>) -> Result<(), Error> {
    output.clear();
    let mut cursor = 0;
    while cursor < diff.len() {
        let unchanged = count(diff[cursor])?;
        cursor += 1;
        let end = output
            .len()
            .checked_add(unchanged)
            .ok_or(Error("diff offset overflow"))?;
        let source = old
            .get(output.len()..end)
            .ok_or(Error("unchanged run exceeds source"))?;
        output.extend_from_slice(source);
        if cursor == diff.len() {
            break;
        }
        let replaced = count(diff[cursor])?;
        cursor += 1;
        let end = cursor
            .checked_add(replaced)
            .ok_or(Error("replacement length overflow"))?;
        let values = diff
            .get(cursor..end)
            .ok_or(Error("truncated replacement run"))?;
        output.extend_from_slice(values);
        cursor = end;
    }
    Ok(())
}

fn count(value: i64) -> Result<usize, Error> {
    nonnegative(value)?;
    usize::try_from(value).map_err(|_| Error("diff count overflow"))
}
