select
    c.id,
    c.name,
    c.email,
    count(o.id) as order_count,
    coalesce(sum(o.amount), 0) as total_spent
from "warehouse"."main"."raw_customers" c
left join "warehouse"."main"."raw_orders" o on c.id = o.customer_id
group by c.id, c.name, c.email