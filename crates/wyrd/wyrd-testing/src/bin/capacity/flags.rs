//! The hand-parsed flags the workloads share.
//!
//! Each workload reads its command line, the program name first, with these
//! helpers: a flag's value, a comma-separated selection of known values, and
//! a check that nothing unknown was passed. Every refusal names the flag and
//! the offending value, so a mistyped selection fails before any server
//! starts.

/// The value after `flag`, or `None` when `flag` is absent.
///
/// # Errors
///
/// Returns a message when `flag` is last (it has no value) or given twice.
pub fn value<'a>(arguments: &'a [String], flag: &str) -> Result<Option<&'a str>, String> {
    let mut found = None;
    for (index, argument) in arguments.iter().enumerate() {
        if argument != flag {
            continue;
        }
        if found.is_some() {
            return Err(format!("{flag} is given twice"));
        }
        let next = arguments
            .get(index + 1)
            .filter(|next| !next.starts_with("--"))
            .ok_or_else(|| format!("{flag} needs a value"))?;
        found = Some(next.as_str());
    }
    Ok(found)
}

/// The comma-separated selection after `flag`, each item read by `parse`,
/// or `None` when `flag` is absent; `expected` describes valid items.
///
/// # Errors
///
/// Returns a message for a missing value, an item `parse` refuses, an empty
/// item, or a duplicate.
pub fn list<T: PartialEq>(
    arguments: &[String],
    flag: &str,
    expected: &str,
    parse: impl Fn(&str) -> Option<T>,
) -> Result<Option<Vec<T>>, String> {
    let Some(list) = value(arguments, flag)? else {
        return Ok(None);
    };
    let mut selected = Vec::new();
    for item in list.split(',') {
        let parsed =
            parse(item.trim()).ok_or_else(|| format!("{flag} takes {expected}, got `{item}`"))?;
        if selected.contains(&parsed) {
            return Err(format!("{flag} names `{item}` twice"));
        }
        selected.push(parsed);
    }
    Ok(Some(selected))
}

/// The client counts `--clients` selects from `matrix`, or all of `matrix`.
///
/// # Errors
///
/// Returns a message for an invalid or duplicate count, or a count outside
/// `matrix`.
pub fn clients(arguments: &[String], matrix: &[usize]) -> Result<Vec<usize>, String> {
    let expected = format!("client counts from {matrix:?}");
    Ok(list(arguments, "--clients", &expected, |item| {
        item.parse().ok().filter(|count| matrix.contains(count))
    })?
    .unwrap_or_else(|| matrix.to_vec()))
}

/// Refuses any argument after the program name that is neither one of
/// `valued` flags (and its value) nor one of `switches`.
///
/// # Errors
///
/// Returns a message naming the first unknown argument.
pub fn known(arguments: &[String], valued: &[&str], switches: &[&str]) -> Result<(), String> {
    let mut rest = arguments.iter().skip(1);
    while let Some(argument) = rest.next() {
        if valued.contains(&argument.as_str()) {
            rest.next();
        } else if !switches.contains(&argument.as_str()) {
            return Err(format!("unknown argument `{argument}`"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{clients, known, list, value};

    /// A command line of `arguments` after the program name.
    fn line(arguments: &[&str]) -> Vec<String> {
        std::iter::once("capacity")
            .chain(arguments.iter().copied())
            .map(str::to_owned)
            .collect()
    }

    /// Flags read their value and refuse a missing, repeated, invalid,
    /// duplicate, or unknown one.
    ///
    /// # Panics
    ///
    /// Panics when a flag parses the wrong way.
    #[test]
    fn flags_refuse_missing_repeated_and_unknown_values() {
        assert_eq!(value(&line(&["--case", "2"]), "--case"), Ok(Some("2")));
        assert_eq!(value(&line(&[]), "--case"), Ok(None));
        assert!(value(&line(&["--case"]), "--case").is_err());
        assert!(value(&line(&["--case", "--no-profile"]), "--case").is_err());
        assert!(value(&line(&["--case", "1", "--case", "2"]), "--case").is_err());
        let digit = |item: &str| item.parse::<u8>().ok();
        assert_eq!(
            list(&line(&["--case", "2,1"]), "--case", "cases", digit),
            Ok(Some(vec![2, 1]))
        );
        assert!(list(&line(&["--case", "1,1"]), "--case", "cases", digit).is_err());
        assert!(list(&line(&["--case", "1,"]), "--case", "cases", digit).is_err());
        assert_eq!(clients(&line(&[]), &[1, 4]), Ok(vec![1, 4]));
        assert_eq!(clients(&line(&["--clients", "4"]), &[1, 4]), Ok(vec![4]));
        assert!(clients(&line(&["--clients", "8"]), &[1, 4]).is_err());
        assert!(
            known(
                &line(&["--case", "1", "--no-profile"]),
                &["--case"],
                &["--no-profile"]
            )
            .is_ok()
        );
        assert!(known(&line(&["--rates", "1"]), &["--case"], &["--no-profile"]).is_err());
    }
}
