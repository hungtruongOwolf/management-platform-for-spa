-- 0029: "Nhật ký ghi chú" reads why a booking was cancelled or the customer did not come
-- (F21, live test 2026-11-08). No new column: the reason already lives in
-- `appointment.status_reason` (required for both statuses). This partial index keeps the new
-- branch of `GET /notes` a short range scan over the closed bookings that carry one.

CREATE INDEX appointment_closed_reason_idx ON app.appointment (tenant_id, closed_at)
  WHERE status IN ('cancelled', 'no_show');
