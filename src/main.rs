use actix_web::{web, App, HttpServer, HttpResponse, Responder};
use dotenvy::dotenv;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use std::env;
use actix_web::dev::Path;
use actix_web::http::header::q;
use serde_json::json;
use sqlx::postgres::PgRow;

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    dotenv().ok();

    let database_url = env::var("DATABASE_URL").expect("DATABASE_URL not found in env file");
    let server_addr = env::var("SERVER_ADDR").expect("SERVER_ADDR not found in env file");

    let pool = PgPool::connect(&database_url)
        .await
        .expect("Failed to create database pool");

    HttpServer::new(move || {
        App::new()
            .app_data(web::Data::new(pool.clone()))
            .route("/", web::get().to(home_page))
            .route("/todos", web::get().to(get_todos))
            .route("/todos", web::post().to(create_todo))
            .route("/register", web::post().to(create_user))
            .route("/todos/{todo_id}", web::patch().to(update_todo))
            .route("/user/{user_id}", web::patch().to(update_user))
            .route("/todos/{todo_id}", web::delete().to(delete_todo))
            .route("/users/{user_id}", web::delete().to(delete_user))
    })
        .bind(&server_addr)?
        .run()
        .await
}

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
struct Todo {
    id: Option<i32>,
    title: Option<String>,
    completed: Option<bool>,
    description: Option<String>,
}


#[derive(Deserialize, Serialize)]
struct UpdateTaskReq {
    title: Option<String>,
    completed: Option<bool>,
    description: Option<String>,
}

#[derive(Deserialize,Serialize)]
struct UpdateUserReq {
    name: Option<String>, // Optional field for updating
    password: Option<String>, // Optional field for updating
}

#[derive(Serialize)]
struct TodoResponse {
    id: i32,
    title: String,
    completed: bool,
    description: String,
}

#[derive(Deserialize)]
struct NewUser {
    name: String,
    password: String,
}
#[derive(Serialize)]
struct UserResponse {
    id: i32,
    name: String,
}

#[derive(Serialize)]
struct User {
    id: i32,
    name: String,
    password: String,
}
// Handler for fetching todos
async fn get_todos(pool: web::Data<PgPool>) -> impl Responder {
    let todos = sqlx::query_as::<_, Todo>("SELECT * FROM todos")
        .fetch_all(pool.get_ref())
        .await
        .expect("Failed to fetch todos");

    HttpResponse::Ok().json(todos)
}

// Handler for updating a todo
async fn update_todo(
    pool: web::Data<PgPool>,
    todo_data: web::Json<UpdateTaskReq>,
    todo_id: web::Path<i32>,
) -> Result<HttpResponse, actix_web::Error> {
    let todo_id = todo_id.into_inner();

    // SQL query to update title, completed, and description, excluding the id
    let result = sqlx::query(
        "UPDATE todos SET title = $1, completed = $2, description = $3 WHERE id = $4"
    )
        .bind(todo_data.title.clone().unwrap_or_else(|| "Untitled".to_string())) // Title or default
        .bind(todo_data.completed.unwrap_or(false))                             // Completed status or default
        .bind(todo_data.description.clone().unwrap_or_else(|| "".to_string()))   // Description or default
        .bind(todo_id)                                                           // Bind the todo_id to ensure we don't change it
        .execute(pool.get_ref())
        .await;

    match result {
        Ok(_) => {
            // Fetch the updated todo to return it in the response
            let updated_todo = sqlx::query_as::<_, Todo>("SELECT * FROM todos WHERE id = $1")
                .bind(todo_id)
                .fetch_one(pool.get_ref())
                .await
                .map_err(|e| actix_web::error::ErrorInternalServerError(e.to_string()))?;

            Ok(HttpResponse::Ok().json(updated_todo)) // Return updated todo
        }
        Err(e) => Err(actix_web::error::ErrorInternalServerError(e.to_string())), // Handle error
    }
}

async fn create_user(
    pool:web::Data<PgPool>,
    new_user: web::Json<NewUser>
) -> Result<HttpResponse, actix_web::Error> {
    let query = sqlx::query!(
    r#"INSERT INTO "Users" (name, password) VALUES ($1, $2) RETURNING id"#,
    new_user.name,
    new_user.password,
)
        .fetch_one(pool.get_ref())
        .await;
    match query {
        Ok(row) => {
            let user_id = row.id; // Assuming the returned row has an `id` field
            let row = sqlx::query!("SELECT id, name FROM \"Users\" WHERE id = $1", user_id) // Only select the fields you need
                .fetch_one(pool.get_ref())
                .await
                .map_err(|e| {
                    eprintln!("Error fetching user: {}", e);
                    actix_web::error::ErrorInternalServerError("Database query failed")
                })?;

            // Map the row to the UserResponse struct
            let user_response = UserResponse {
                id: row.id,
                name: row.name,
            };

            Ok(HttpResponse::Created().json(user_response))
        }
        Err(e) => {
            // Handle the error (you can log it, etc.)
            eprintln!("Failed to create user: {:?}", e);
            Err(actix_web::error::ErrorInternalServerError("Failed to create user"))
        }
    }
}

async fn delete_user(
    pool: web::Data<PgPool>,
    user_id: web::Path<i32>
) -> Result<HttpResponse, actix_web::Error> {
    let user_id = user_id.into_inner();
    let existing_user = sqlx::query!("SELECT * FROM \"Users\" WHERE id = $1", user_id)
        .fetch_optional(pool.get_ref())
        .await;

    match existing_user {
        Ok(Some(_)) => {
            // If user exists, proceed to delete
            let query = sqlx::query!("DELETE FROM \"Users\" WHERE id = $1", user_id)
                .execute(pool.get_ref())
                .await;

            match query {
                Ok(_) => {
                    Ok(HttpResponse::Ok().body("User successfully deleted"))
                },
                Err(e) => {
                    eprintln!("Failed to delete user: {:?}", e);
                    Err(actix_web::error::ErrorInternalServerError("Failed to delete user"))
                }
            }
        },
        Ok(None) => {
            // If no user is found with the given ID
            Ok(HttpResponse::NotFound().body("User not found"))
        },
        Err(e) => {
            eprintln!("Error checking user existence: {:?}", e);
            Err(actix_web::error::ErrorInternalServerError("Error checking user existence"))
        }
    }
}
async fn update_user(
    pool: web::Data<PgPool>,
    user_id: web::Path<i32>,
    user_data: web::Json<UpdateUserReq>,
) -> Result<HttpResponse, actix_web::Error> {
    let user_id = user_id.into_inner();

    // First, check if the user exists
    let existing_user = sqlx::query_as!(
        User,
        "SELECT id, name, password FROM \"Users\" WHERE id = $1",
        user_id
    )
        .fetch_optional(pool.get_ref())
        .await
        .map_err(|e| {
            eprintln!("Error fetching user: {:?}", e);
            actix_web::error::ErrorInternalServerError("Database query failed")
        })?;

    // If the user does not exist, return a 404 response
    if existing_user.is_none() {
        return Ok(HttpResponse::NotFound().body("User not found"));
    }

    // Proceed to update the user
    let query = sqlx::query!(
        "UPDATE \"Users\" SET name = COALESCE($1, name), password = COALESCE($2, password) WHERE id = $3",
        user_data.name.as_deref(),  // Use as_deref to convert Option<String> to Option<&str>
        user_data.password.as_deref(),
        user_id
    )
        .execute(pool.get_ref())
        .await
        .map_err(|e| {
            eprintln!("Error updating user: {:?}", e);
            actix_web::error::ErrorInternalServerError("Database query failed")
        })?;

    // Check if any rows were affected
    if query.rows_affected() == 0 {
        return Ok(HttpResponse::NotFound().body("User not found")); // Return 404 if no rows were affected
    }

    // Fetch the updated user to return
    let updated_user = sqlx::query_as!(User, "SELECT id, name, password FROM \"Users\" WHERE id = $1", user_id)
        .fetch_one(pool.get_ref())
        .await
        .map_err(|e| {
            eprintln!("Error fetching updated user: {:?}", e);
            actix_web::error::ErrorInternalServerError("Database query failed")
        })?;

    // Return the updated user as JSON
    Ok(HttpResponse::Ok().json(updated_user)) // Returning updated user
}




// Handler for deleting a todo
async fn delete_todo(
    pool: web::Data<PgPool>,
    todo_id: web::Path<i32>,  // Don't destructure here
) -> impl Responder {
    let todo_id = todo_id.into_inner();  // Extract the value here
    let result = sqlx::query!("DELETE FROM todos WHERE id = $1", todo_id)
        .execute(pool.get_ref())
        .await;

    match result {
        Ok(_) => HttpResponse::NoContent().finish(),
        Err(e) => HttpResponse::InternalServerError().json(format!("Failed to delete todo: {}", e)),
    }
}


// Handler for creating a new todo
async fn create_todo(
    pool: web::Data<PgPool>,
    new_todo: web::Json<Todo>,
) -> Result<HttpResponse, actix_web::Error> {
    let row = sqlx::query!(
        r#"INSERT INTO todos (title, completed, description) VALUES ($1, $2, $3) RETURNING id, title, completed, description"#,
        new_todo.title.clone().unwrap_or_else(|| "Untitled".to_string()),
        new_todo.completed.unwrap_or(false),
        new_todo.description.clone().unwrap_or_else(|| "".to_string()),
    )
        .fetch_one(pool.get_ref())
        .await
        .map_err(|e| actix_web::error::ErrorInternalServerError(e.to_string()))?;

    let response = TodoResponse {
        id: row.id,
        title: row.title,
        completed: row.completed,
        description: row.description.unwrap(),
    };

    Ok(HttpResponse::Created().json(response))
}



// Home page handler
async fn home_page() -> impl Responder {
    "Welcome to the Todo API"
}


