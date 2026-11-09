-- Read-only: "who did it" columns that hold a value which is not a user_account id (db review
-- C4). Zero everywhere = an FK can be added and validated.
SELECT 'visit.created_by' AS col, count(*) FROM app.visit x WHERE NOT EXISTS (SELECT 1 FROM app.user_account u WHERE u.id = x.created_by)
UNION ALL SELECT 'visit.voided_by', count(*) FROM app.visit x WHERE x.voided_by IS NOT NULL AND NOT EXISTS (SELECT 1 FROM app.user_account u WHERE u.id = x.voided_by)
UNION ALL SELECT 'sale.created_by', count(*) FROM app.sale x WHERE NOT EXISTS (SELECT 1 FROM app.user_account u WHERE u.id = x.created_by)
UNION ALL SELECT 'sale.voided_by', count(*) FROM app.sale x WHERE x.voided_by IS NOT NULL AND NOT EXISTS (SELECT 1 FROM app.user_account u WHERE u.id = x.voided_by)
UNION ALL SELECT 'payment.created_by', count(*) FROM app.payment x WHERE NOT EXISTS (SELECT 1 FROM app.user_account u WHERE u.id = x.created_by)
UNION ALL SELECT 'refund.created_by', count(*) FROM app.refund x WHERE NOT EXISTS (SELECT 1 FROM app.user_account u WHERE u.id = x.created_by)
UNION ALL SELECT 'customer_package.created_by', count(*) FROM app.customer_package x WHERE NOT EXISTS (SELECT 1 FROM app.user_account u WHERE u.id = x.created_by)
UNION ALL SELECT 'customer_package.cancelled_by', count(*) FROM app.customer_package x WHERE x.cancelled_by IS NOT NULL AND NOT EXISTS (SELECT 1 FROM app.user_account u WHERE u.id = x.cancelled_by)
UNION ALL SELECT 'appointment.created_by', count(*) FROM app.appointment x WHERE NOT EXISTS (SELECT 1 FROM app.user_account u WHERE u.id = x.created_by)
UNION ALL SELECT 'appointment.closed_by', count(*) FROM app.appointment x WHERE x.closed_by IS NOT NULL AND NOT EXISTS (SELECT 1 FROM app.user_account u WHERE u.id = x.closed_by);
