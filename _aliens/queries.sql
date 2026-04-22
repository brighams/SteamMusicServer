
select * from steam_app_details where date_updated is null order by title
;

select * from steam_app_details where parent_id is not null order by title;



select count(*) from (select sad1.appid, sad1.title,  sad2.appid, sad2.title
from steam_app_details as sad1
         left join steam_app_details as sad2 on sad1.appid = sad2.parent_id
where sad1.parent_id is null and sad2.parent_id is not null);


select sad1.appid, sad1.title,  sad2.appid, sad2.title
from steam_app_details as sad1
         left join steam_app_details as sad2 on sad1.appid = sad2.parent_id
where sad1.parent_id is null and sad2.parent_id is not null;
