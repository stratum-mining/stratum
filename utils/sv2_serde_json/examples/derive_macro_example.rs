use sv2_serde_json_macros::{SerJson, DeJson};
use sv2_serde_json::value::{Value, Number};

#[derive(SerJson, DeJson, Debug)]
struct Person {
    name: String,
    age: i64,
    is_student: bool,
}

#[derive(SerJson, DeJson, Debug)]
enum Status {
    Active,
    Inactive,
    Pending,
}

fn main() {
    let person = Person {
        name: "Alice".to_string(),
        age: 25,
        is_student: true,
    };

    let person_json = person.to_json_value();
    println!("Serialized: {:?}", person_json.to_json_string());

    let deserialized_person = Person::from_json_value(person_json).unwrap();
    println!("Deserialized: {:?}", deserialized_person);

    let status = Status::Active;
    let status_json = status.to_json_value();
    println!("Serialized Enum: {:?}", status_json);

    let deserialized_status = Status::from_json_value(status_json).unwrap();
    println!("Deserialized Enum: {:?}", deserialized_status);
}