use std::path::Path;

use super::RegistryMigration;
use crate::prelude::*;

pub struct CategoryOrder;
impl RegistryMigration for CategoryOrder {
    fn action(&self, db: &mut Value, _: &Path) -> Result<(), Error> {
        if let Some(categories) = db
            .get_mut("index")
            .and_then(|index| index.get_mut("package"))
            .and_then(|package| package.get_mut("categories"))
            .and_then(Value::as_object_mut)
        {
            populate_indices(categories.iter_mut().map(|(_, category)| category))?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use imbl_value::json;

    use super::*;

    #[test]
    fn existing_categories_precede_subsequent_additions() {
        let mut db = json!({ "index": { "package": { "categories": {
            "z": { "name": "First" },
            "a": { "name": "Second" }
        } } } });
        CategoryOrder.action(&mut db, Path::new("")).unwrap();
        let categories = &db["index"]["package"]["categories"];
        assert_eq!(categories["z"]["index"], json!(0));
        assert_eq!(categories["a"]["index"], json!(1));
        assert_eq!(categories["a"]["name"], json!("Second"));
        let migrated = db.clone();
        CategoryOrder.action(&mut db, Path::new("")).unwrap();
        assert_eq!(db, migrated);
        let categories = db["index"]["package"]["categories"]
            .as_object_mut()
            .unwrap();
        let next = next_index(categories.values().map(|category| {
            from_value::<Option<u32>>(category["index"].clone()).unwrap()
        }));
        categories.insert("new".into(), json!({ "name": "Third", "index": next }));
        CategoryOrder.action(&mut db, Path::new("")).unwrap();
        assert_eq!(
            db["index"]["package"]["categories"]["new"]["index"],
            json!(2)
        );
        assert_eq!(
            db["index"]["package"]["categories"]["z"]["index"],
            json!(0)
        );
    }
}
