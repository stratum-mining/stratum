use sv2_serde_json::value::{FromJsonValue, ToJsonValue};
use sv2_serde_json_macros::{DeJson, SerJson};

#[derive(SerJson, DeJson, Debug, Clone)]
struct Person {
    name: String,
    age: i64,
    is_student: bool,
    here: Vec<String>,
    check: sv2_serde_json::value::Value,
    optional_check: Option<sv2_serde_json::value::Value>,
    non_optional_check: Option<String>,
}

#[derive(SerJson, DeJson, Debug)]
struct Person2 {
    person: Person,
    optional_struct: Option<Person>,
}

#[derive(SerJson, DeJson, Debug)]
enum Status {
    Active,
    Inactive,
    Pending,
    Here(Person),
}

fn main() {
    let person = Person {
        name: "Alice".to_string(),
        age: 25,
        is_student: true,
        here: vec!["HEre".to_string()],
        check: sv2_serde_json::value::Value::Null,
        optional_check: Some(sv2_serde_json::value::Value::Null),
        non_optional_check: Some("Hello".to_string()),
    };

    let person2 = Person2 {
        person: person.clone(),
        optional_struct: Some(person.clone()),
    };

    let person_json = person.to_json_value();
    println!("Serialized: {:?}", person_json.to_json_string());

    let deserialized_person = Person::from_json_value(&person_json).unwrap();
    println!("Deserialized: {:?}", deserialized_person);

    let person2_json = person2.to_json_value();
    println!("Serialized: {:?}", person2_json.to_json_string());

    let deserialized_person2 = Person2::from_json_value(&person2_json).unwrap();
    println!("Deserialized: {:?}", deserialized_person2);

    let status = Status::Active;
    let status_json = status.to_json_value();
    println!("Serialized Enum: {:?}", status_json);

    let deserialized_status = Status::from_json_value(&status_json);
    println!("Deserialized Enum: {:?}", deserialized_status);
}
