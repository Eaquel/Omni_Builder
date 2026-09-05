pub const CATALOGUE: &[(&str, &str)] = &[
    ("EJ001", "A comment was opened and never closed."),
    (
        "EJ002",
        "A character that is not part of Java appeared in the source.",
    ),
    (
        "EJ003",
        "A unicode escape needs exactly four hexadecimal digits.",
    ),
    ("EJ004", "That backslash escape is not one Java knows."),
    (
        "EJ005",
        "A string or a text block was opened and never closed.",
    ),
    (
        "EJ006",
        "An escape or a character literal is not shaped the way Java writes one.",
    ),
    (
        "EJ007",
        "A number with a leading zero is octal, where 8 and 9 are not digits.",
    ),
    ("EJ008", "That number is too large to be a long."),
    ("EJ009", "That is not a number this reads."),
    ("EJ010", "That number is too large to be an int."),
    (
        "EJ011",
        "A text block opens with three quotes and then a line break.",
    ),
    ("EJ100", "A mark the language expects here is not there."),
    ("EJ101", "A name was expected here."),
    ("EJ103", "A type was expected here."),
    (
        "EJ104",
        "A class or an interface was expected, or the file declares nothing.",
    ),
    ("EJ105", "A declaration was opened and never closed."),
    ("EJ106", "A method has no body and is not marked abstract."),
    ("EJ107", "A method body was opened and never closed."),
    ("EJ108", "A block was never closed."),
    (
        "EJ109",
        "A statement is not shaped the way the language writes one.",
    ),
    ("EJ110", "A switch was never closed."),
    (
        "EJ111",
        "A switch holds case and default arms and nothing else.",
    ),
    (
        "EJ112",
        "A switch uses an arrow for every arm or a colon for every arm, not both.",
    ),
    ("EJ113", "A switch has one default arm at most."),
    (
        "EJ114",
        "A try needs a catch, a finally, or something to close.",
    ),
    (
        "EJ115",
        "A list of type arguments or type parameters was opened and never closed.",
    ),
    (
        "EJ116",
        "There is something after the last declaration in this file.",
    ),
    (
        "EJ117",
        "A record names the same thing as a component and as a field.",
    ),
    (
        "EJ118",
        "A try with brackets holds at least one thing to close, and each is given a value.",
    ),
    (
        "EJ119",
        "Compiling this keeps producing classes that produce more classes.",
    ),
    (
        "EJ120",
        "An array needs a size, and a size cannot follow an empty pair of brackets.",
    ),
    (
        "EJ121",
        "Something was used as an array that is not one, or holds another kind of thing.",
    ),
    ("EJ122", "Only a class can be named in this position."),
    ("EJ200", "A name is not a type this compilation knows."),
    (
        "EJ201",
        "A value of this type does not become the type that is wanted.",
    ),
    (
        "EJ202",
        "There is no instance here, so this has no meaning in a static method.",
    ),
    ("EJ203", "Something that is not an array was indexed."),
    (
        "EJ204",
        "A value of this type cannot be negated or have its bits flipped.",
    ),
    ("EJ205", "Not wants a boolean and was given something else."),
    ("EJ206", "A condition has to be a boolean."),
    (
        "EJ207",
        "The two sides of a conditional do not meet in one type.",
    ),
    (
        "EJ208",
        "A pattern does not match the thing it is taking apart.",
    ),
    ("EJ209", "An array of void is not a thing."),
    (
        "EJ210",
        "An instance member was reached from a static method or through the class name.",
    ),
    ("EJ211", "A name is not anything this method can see."),
    ("EJ212", "A value of this type has no fields."),
    (
        "EJ213",
        "A class has no field of that name that this compilation knows.",
    ),
    ("EJ214", "Both sides of an and or an or want booleans."),
    (
        "EJ215",
        "There is no operator between values of these two types.",
    ),
    ("EJ216", "That operator wants integers or booleans."),
    (
        "EJ217",
        "Objects can only be compared with equals and not-equals here.",
    ),
    ("EJ218", "Making something new wants a class."),
    (
        "EJ219",
        "A class has no constructor taking that many arguments.",
    ),
    (
        "EJ220",
        "A call was given a number of arguments it does not take.",
    ),
    (
        "EJ221",
        "An argument of this type cannot be given where another is wanted.",
    ),
    (
        "EJ222",
        "There is no instance here, so super and this have no meaning.",
    ),
    ("EJ223", "This class extends nothing to call up into."),
    (
        "EJ224",
        "There is no method of that name and that many arguments on this class or one above it.",
    ),
    (
        "EJ225",
        "A method that belongs to an instance was called from a static one.",
    ),
    ("EJ226", "A value of this type has no method of that name."),
    (
        "EJ227",
        "A class has no method of that name and arity that this compilation knows.",
    ),
    (
        "EJ228",
        "A value of this type cannot be put in a variable of another.",
    ),
    ("EJ229", "That is nothing which can be assigned to."),
    (
        "EJ230",
        "A variable of one type was given a value of another.",
    ),
    (
        "EJ231",
        "A break, a continue or a label is not inside anything it can leave.",
    ),
    (
        "EJ232",
        "A method can reach its end without returning a value.",
    ),
    (
        "EJ233",
        "A return carries a value where none is wanted, or none where one is.",
    ),
    (
        "EJ234",
        "A variable written with var has nothing to take its type from.",
    ),
    (
        "EJ235",
        "Throwing wants a Throwable and was given something else.",
    ),
    (
        "EJ236",
        "A for cannot walk over this, or what it hands over is not an iterator.",
    ),
    (
        "EJ237",
        "The loop variable does not match what is walked over.",
    ),
    (
        "EJ238",
        "A switch was given something it cannot take, or a pattern that is not a class.",
    ),
    (
        "EJ239",
        "A case answers to a constant of the kind the switch is over.",
    ),
    ("EJ240", "The same case is written twice in one switch."),
    ("EJ241", "A catch wants a class."),
    (
        "EJ242",
        "A call to super or this may only be the first statement of a constructor.",
    ),
    (
        "EJ243",
        "This class has no constructor taking that many arguments.",
    ),
    (
        "EJ244",
        "A yield is not inside a switch used for its value.",
    ),
    (
        "EJ245",
        "A switch used for its value needs a default, because it has to answer for anything.",
    ),
    (
        "EJ246",
        "Every arm of a switch used for its value has to produce one.",
    ),
    (
        "EJ247",
        "The arms of a switch used for its value do not meet in one type.",
    ),
    (
        "EJ248",
        "What is written where a class is used is not a class or an interface.",
    ),
    (
        "EJ249",
        "A class written where it is used cannot hold that.",
    ),
    (
        "EJ250",
        "There is nothing here a lambda or a method reference can stand for.",
    ),
    (
        "EJ251",
        "A lambda or a method reference does not match the shape it stands for.",
    ),
    (
        "EJ252",
        "An instance was named where there is none, or an enclosing one cannot be reached.",
    ),
    (
        "EJ253",
        "A type is declared twice, or a lock was given something that is not an object.",
    ),
    (
        "EJ254",
        "A module declaration is not something Android reads.",
    ),
    (
        "EJ255",
        "That archive is not one this reads, or it holds no class files.",
    ),
    (
        "EJ256",
        "A checked exception has to be caught or declared to be thrown.",
    ),
    (
        "EJ257",
        "A class that is not abstract does not implement a method its supertypes leave abstract.",
    ),
    (
        "EJ262",
        "A call could be more than one method, and neither is more specific than the other.",
    ),
    (
        "EJ300",
        "Java is compiled through the compiler contract, not through this one.",
    ),
    (
        "EJ301",
        "The chosen Android API is outside what this builds for.",
    ),
    ("EJ302", "A Java source file is not text."),
    ("EJ900", "That language is not compiled here."),
];

pub fn english_of(code: &str) -> Option<&'static str> {
    CATALOGUE
        .binary_search_by(|(held, _)| (*held).cmp(code))
        .ok()
        .map(|at| CATALOGUE[at].1)
}

pub fn explain(code: &str) -> Option<String> {
    english_of(code).map(crate::speech::sentence)
}

pub fn known(code: &str) -> bool {
    english_of(code).is_some()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_catalogue_is_sorted_and_holds_no_code_twice() {
        let mut seen: Vec<&str> = CATALOGUE.iter().map(|(code, _)| *code).collect();
        let ordered = seen.clone();
        seen.sort_unstable();
        seen.dedup();
        assert_eq!(seen.len(), CATALOGUE.len(), "a code is catalogued twice");
        assert_eq!(
            ordered, seen,
            "the catalogue is not in order, so a lookup would miss"
        );
    }

    #[test]
    fn every_code_the_java_compiler_raises_is_explained() {
        let source = include_str!("Java.rs");
        let mut raised: Vec<String> = Vec::new();
        let held: Vec<char> = source.chars().collect();
        let mut at = 0usize;
        while at + 7 <= held.len() {
            if held[at] == '"' && held[at + 1] == 'E' && held[at + 2] == 'J' && held[at + 6] == '"'
            {
                let digits: String = held[at + 3..at + 6].iter().collect();
                if digits.chars().all(|one| one.is_ascii_digit()) {
                    raised.push(format!("EJ{digits}"));
                }
            }
            at += 1;
        }
        raised.sort();
        raised.dedup();
        assert!(!raised.is_empty(), "no codes were read out of the compiler");
        let missing: Vec<&String> = raised.iter().filter(|code| !known(code)).collect();
        assert!(
            missing.is_empty(),
            "these codes are raised and not explained: {missing:?}"
        );
    }

    #[test]
    fn an_explanation_is_given_in_the_chosen_language() {
        let _turn = crate::progress::one_at_a_time();
        crate::speech::choose("en");
        assert_eq!(
            explain("EJ257").as_deref(),
            Some("A class that is not abstract does not implement a method its supertypes leave abstract.")
        );
        crate::speech::choose("tr");
        let turkish = explain("EJ257").expect("the code is catalogued");
        crate::speech::choose("en");
        assert!(turkish.contains("abstract olmayan"), "{turkish}");
        assert_eq!(explain("EJ999"), None);
    }
}
