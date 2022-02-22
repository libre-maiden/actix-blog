#[macro_use]
extern crate diesel;
pub mod schema;
pub mod models;

use actix_web::{HttpServer, App, web, HttpResponse, Responder};
use tera::{Tera, Context};
use serde::{Deserialize};
use actix_identity::{Identity, CookieIdentityPolicy, IdentityService};

use diesel::prelude::*;
use diesel::pg::PgConnection;
use dotenv::dotenv;

use models::{User, NewUser, LoginUser, Post, NewPost, Comment, NewComment};


#[derive(Deserialize)]
struct PostForm {
    title: String,
    link: String,
}

#[derive(Deserialize)]
struct CommentForm {
    comment: String,
}

fn establish_connection() -> PgConnection {
    dotenv().ok();

    let database_url = std::env::var("DATABASE_URL")
        .expect("DATABASE_URL must be set");

    PgConnection::establish(&database_url)
        .expect(&format!("Error connecting to {}", database_url))
}

async fn index(tera: web::Data<Tera>) -> impl Responder {
    use schema::posts::dsl::{posts};
    use schema::users::dsl::{users};

    let connection = establish_connection();
    let all_posts :Vec<(Post, User)> = posts.inner_join(users)
        .load(&connection)
        .expect("Error retrieving all posts.");

    let mut data = Context::new();
    data.insert("title", "Blog");
    data.insert("posts_users", &all_posts);

    let rendered = tera.render("index.html", &data).unwrap();
    HttpResponse::Ok().body(rendered)
}

async fn signup(tera: web::Data<Tera>) -> impl Responder {
    let mut data = Context::new();
    data.insert("title", "Sign Up");

    let rendered = tera.render("signup.html", &data).unwrap();
    HttpResponse::Ok().body(rendered)
}

async fn login(tera: web::Data<Tera>, id: Identity) -> impl Responder {
    let mut data = Context::new();
    data.insert("title", "Login");

    if let Some(id) = id.identity() {
        return HttpResponse::Ok().body("Already logged in.")
    }

    let rendered = tera.render("login.html", &data).unwrap();
    HttpResponse::Ok().body(rendered)
}

async fn submission(tera: web::Data<Tera>) -> impl Responder {
    let mut data = Context::new();
    data.insert("title", "Submit a Post");

    let rendered = tera.render("submission.html", &data).unwrap();
    HttpResponse::Ok().body(rendered)
}

async fn process_signup(data: web::Form<NewUser>) -> impl Responder {
    use schema::users;

    let connection = establish_connection();

    diesel::insert_into(users::table)
        .values(&*data)
        .get_result::<User>(&connection)
        .expect("Error registering user.");

    println!("{:?}", data);
    HttpResponse::Ok().body(format!("Successfully saved user: {}", data.username))
}

async fn process_login(data: web::Form<LoginUser>, id: Identity) -> impl Responder {
    use schema::users::dsl::{username, users};

    let connection = establish_connection();
    let user = users.filter(username.eq(&data.username)).first::<User>(&connection);

    match user {
        Ok(u) => {
            if u.password == data.password {
                let session_token = String::from(u.username);
                id.remember(session_token);
                HttpResponse::Ok().body(format!("Logged in: {}", data.username))
            } else {
                HttpResponse::Ok().body("Password is incorrect.")
            }
        },
        Err(e) => {
            println!("{:?}", e);
            HttpResponse::Ok().body("User doesn't exist.")
        }
    }
}

async fn process_submission(data: web::Form<PostForm>, id: Identity) -> impl Responder {
    if let Some(id) = id.identity() {
        use schema::users::dsl::{username, users};

        let connection = establish_connection();
        let user :Result<User, diesel::result::Error> = users.filter(username.eq(id)).first(&connection);

        match user {
            Ok(u) => {
                let new_post = NewPost::from_post_form(data.title.clone(), data.link.clone(), u.id);

                use schema::posts;

                diesel::insert_into(posts::table)
                    .values(&new_post)
                    .get_result::<Post>(&connection)
                    .expect("Error saving post.");

                return HttpResponse::Ok().body("Submitted.");
            }
            Err(e) => {
                println!("{:?}", e);
                return HttpResponse::Ok().body("Failed to find user.");
            }
        }
    }
    HttpResponse::Unauthorized().body("User not logged in.")
}

async fn logout(id: Identity) -> impl Responder {
    id.forget();
    HttpResponse::Ok().body("Logged out.")
}

async fn post_page(tera: web::Data<Tera>,
    id: Identity,
    web::Path(post_id): web::Path<i32>
) -> impl Responder {
    use schema::posts::dsl::{posts};
    use schema::users::dsl::{users};

    let connection = establish_connection();

    let post :Post = posts.find(post_id)
        .get_result(&connection)
        .expect("Failed to find post.");

    let user :User = users.find(post.author)
        .get_result(&connection)
        .expect("Failed to find user.");

    let comments :Vec<(Comment, User)> = Comment::belonging_to(&post)
        .inner_join(users)
        .load(&connection)
        .expect("Failed to find comments.");    
        
    let mut data = Context::new();
    data.insert("title", &format!("{} - Blog", post.title));
    data.insert("post", &post);
    data.insert("user", &user);
    data.insert("comments", &comments);
    
    if let Some(_id) = id.identity() {
        data.insert("logged_in", "true")
    } else {
        data.insert("logged_in", "false");
    }

    let rendered = tera.render("post.html", &data).unwrap();
    HttpResponse::Ok().body(rendered)
}

async fn comment(
    data: web::Form<CommentForm>,
    id: Identity,
    web::Path(post_id): web::Path<i32>
) -> impl Responder {

    if let Some(id) = id.identity() {
        use schema::posts::dsl::{posts};
        use schema::users::dsl::{users, username};

        let connection = establish_connection();

        let post :Post = posts.find(post_id)
            .get_result(&connection)
            .expect("Failed to find post.");

        let user :Result<User, diesel::result::Error> = users
            .filter(username.eq(id))
            .first(&connection);

        match user {
            Ok(u) => {
                let parent_id = None;
                let new_comment = NewComment::new(data.comment.clone(), post.id, u.id, parent_id);

                use schema::comments;
                diesel::insert_into(comments::table)
                    .values(&new_comment)
                    .get_result::<Comment>(&connection)
                    .expect("Error saving comment.");


                return HttpResponse::Ok().body("Commented.");
            }
            Err(e) => {
                println!("{:?}", e);
                return HttpResponse::Ok().body("User not found.");
            }
        }
    }

    HttpResponse::Unauthorized().body("Not logged in.")
}

#[actix_web::main]
async fn main() -> std::io::Result<()> {
    HttpServer::new(|| {
        let tera = Tera::new("templates/**/*").unwrap();
        App::new()
            .wrap(IdentityService::new(
                    CookieIdentityPolicy::new(&[0;32])
                    .name("auth-cookie")
                    .secure(false)
                )
            )
            .data(tera)
            .route("/", web::get().to(index))
            .route("/signup", web::get().to(signup))
            .route("/signup", web::post().to(process_signup))
            .route("/login", web::get().to(login))
            .route("/login", web::post().to(process_login))
            .route("/logout", web::to(logout))
            .route("/submission", web::get().to(submission))
            .route("/submission", web::post().to(process_submission))
            .service(
                web::resource("/post/{post_id}")
                    .route(web::get().to(post_page))
                    .route(web::post().to(comment))
            )
    })
    .bind("127.0.0.1:8000")?
    .run()
    .await
}
