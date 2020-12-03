use crate::auth;
use crate::diesel::QueryDsl;
use crate::diesel::RunQueryDsl;
use crate::handlers::types::*;
use crate::helpers::{email, email_template};
use crate::model::{MailList, NewUserMail, Space, SpaceUser, User, UserMail};
use crate::schema::maillists::dsl::*;
use crate::schema::spaces::dsl::*;
use crate::schema::spaces_users::dsl::space_id;
use crate::schema::spaces_users::dsl::user_id as space_user_id;
use crate::schema::spaces_users::dsl::*;
use crate::schema::usermails::dsl::user_id as mail_user_id;
use crate::schema::usermails::dsl::*;
use crate::schema::users::dsl::*;
use crate::Pool;

use actix_web::{web, Error, HttpResponse};
use actix_web_httpauth::extractors::bearer::BearerAuth;
use diesel::dsl::{delete, insert_into};
use diesel::prelude::*;

//http calls

pub async fn add_user_folder(
    db: web::Data<Pool>,
    auth: BearerAuth,
    folder: web::Path<AddUserToFolderPath>,
    item: web::Json<AddUserToFoldr>,
) -> Result<HttpResponse, Error> {
    match auth::validate_token(&auth.token().to_string()) {
        Ok(res) => {
            if res == true {
                Ok(web::block(move || {
                    add_user_folder_db(db, auth.token().to_string(), folder, item)
                })
                .await
                .map(|response| HttpResponse::Ok().json(response))
                .map_err(|_| HttpResponse::InternalServerError())?)
            } else {
                Ok(HttpResponse::Ok().json(ResponseError::new(false, "jwt error".to_string())))
            }
        }
        Err(_) => Ok(HttpResponse::Ok().json(ResponseError::new(false, "jwt error".to_string()))),
    }
}

pub async fn remove_user_folder(
    db: web::Data<Pool>,
    auth: BearerAuth,
    folder: web::Path<AddUserToFolderPath>,
    item: web::Json<DeleteMailList>,
) -> Result<HttpResponse, Error> {
    match auth::validate_token(&auth.token().to_string()) {
        Ok(res) => {
            if res == true {
                Ok(web::block(move || {
                    remove_user_folder_db(db, auth.token().to_string(), folder, item)
                })
                .await
                .map(|response| HttpResponse::Ok().json(response))
                .map_err(|_| HttpResponse::InternalServerError())?)
            } else {
                Ok(HttpResponse::Ok().json(ResponseError::new(false, "jwt error".to_string())))
            }
        }
        Err(_) => Ok(HttpResponse::Ok().json(ResponseError::new(false, "jwt error".to_string()))),
    }
}

pub async fn send_mail_to_folder(
    db: web::Data<Pool>,
    folder_id: web::Path<IdPathInfo>,
    item: web::Json<SendMail>,
) -> Result<HttpResponse, Error> {
    Ok(
        web::block(move || send_mail_to_folder_db(db, folder_id, item))
            .await
            .map(|response| HttpResponse::Ok().json(response))
            .map_err(|_| {
                HttpResponse::Ok().json(Response::new(false, "error sending email".to_string()))
            })?,
    )
}

//db calls
fn send_mail_to_folder_db(
    db: web::Data<Pool>,
    folder_id: web::Path<IdPathInfo>,
    item: web::Json<SendMail>,
) -> Result<Response<String>, diesel::result::Error> {
    let conn = db.get().unwrap();

    let mail_list: MailList = maillists.find(folder_id.id).first::<MailList>(&conn)?;

    let user_mail: Vec<(UserMail, User)> = UserMail::belonging_to(&mail_list)
        .inner_join(users)
        .load::<(UserMail, User)>(&conn)?;

    for send_user in user_mail.iter() {
        let template = email_template::notify_folder(&mail_list.folder_name, &item.body);
        email::send_email(
            &send_user.1.email,
            &send_user.1.username,
            &item.title,
            &template,
        );
    }
    Ok(Response::new(
        true,
        "Email sent to all members successfully".to_string(),
    ))
}

fn remove_user_folder_db(
    db: web::Data<Pool>,
    token: String,
    folder: web::Path<AddUserToFolderPath>,
    item: web::Json<DeleteMailList>,
) -> Result<Response<String>, diesel::result::Error> {
    let conn = db.get().unwrap();
    let decoded_token = auth::decode_token(&token);
    let user = users
        .find(decoded_token.parse::<i32>().unwrap())
        .first::<User>(&conn)?;

    let space: Space = spaces
        .filter(spaces_name.ilike(&folder.info))
        .first::<Space>(&conn)?;

    let spaces_user: SpaceUser = spaces_users
        .filter(space_id.eq(space.id))
        .filter(space_user_id.eq(user.id))
        .first::<SpaceUser>(&conn)?;

    if !spaces_user.admin_status {
        return Ok(Response::new(
            false,
            "only admin allowed to add users to folder".to_string(),
        ));
    }
    let folder: MailList = maillists.find(folder.id).first::<MailList>(&conn)?;

    let _count = delete(
        usermails
            .filter(mail_list_id.eq(folder.id))
            .filter(mail_user_id.eq(&item.id)),
    )
    .execute(&conn)?;

    Ok(Response::new(true, "user removed successfully".to_string()))
}

fn add_user_folder_db(
    db: web::Data<Pool>,
    token: String,
    folder: web::Path<AddUserToFolderPath>,
    item: web::Json<AddUserToFoldr>,
) -> Result<Response<String>, diesel::result::Error> {
    let conn = db.get().unwrap();
    let decoded_token = auth::decode_token(&token);
    let user = users
        .find(decoded_token.parse::<i32>().unwrap())
        .first::<User>(&conn)?;

    let space: Space = spaces
        .filter(spaces_name.ilike(&folder.info))
        .first::<Space>(&conn)?;

    let spaces_user: SpaceUser = spaces_users
        .filter(space_id.eq(space.id))
        .filter(space_user_id.eq(user.id))
        .first::<SpaceUser>(&conn)?;

    if !spaces_user.admin_status {
        return Ok(Response::new(
            false,
            "only admin allowed to add users to folder".to_string(),
        ));
    }

    let folder: MailList = maillists.find(folder.id).first::<MailList>(&conn)?;

    for new_user_id in item.id.iter() {
        let user_in_folder = usermails
            .filter(mail_user_id.eq(&new_user_id))
            .filter(mail_list_id.eq(folder.id))
            .first::<UserMail>(&conn);

        match user_in_folder {
            Ok(_user) => {
                //do nothing for user already in folder
            }
            Err(diesel::result::Error::NotFound) => {
                //if user not found
                let new_user = NewUserMail {
                    mail_list_id: &folder.id,
                    user_id: &new_user_id,
                };

                let _res = insert_into(usermails).values(&new_user).execute(&conn)?;
            }
            _ => {
                println!("An error occured");
            }
        }
    }

    Ok(Response::new(true, "Users added successfully".to_string()))
}