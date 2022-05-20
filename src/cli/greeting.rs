use clap::ArgEnum;

#[derive(ArgEnum, Debug, Clone, Copy)]
pub(crate) enum Greeting {
    Hello,
    GoodBye,
}
// impl Greeting {
//     pub fn possible_values() -> impl Iterator<Item = PossibleValue<'static>> {
//         Greeting::value_variants()
//             .iter()
//             .filter_map(ArgEnum::to_possible_value)
//     }
// }
// impl std::fmt::Display for Greeting {
//     fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
//         self.to_possible_value()
//             .expect("no values are skipped")
//             .get_name()
//             .fmt(f)
//     }
// }

// impl std::str::FromStr for Greeting {
//     type Err = String;

//     fn from_str(s: &str) -> Result<Self, Self::Err> {
//         for variant in Self::value_variants() {
//             if variant.to_possible_value().unwrap().matches(s, false) {
//                 return Ok(*variant);
//             }
//         }
//         Err(format!("Invalid variant: {}", s))
//     }
// }
