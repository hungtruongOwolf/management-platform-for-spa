-- Served companions (EM 2026-11-07, spec front-desk-and-packages §10.10, ADR-0009).
--
-- A companion who gets a service is an appointment of their own in the booking's group: own
-- services, own technician (their block on the calendar, same overlap rules), own check-in.
-- They may be known only by a name ("Chồng của chị Hà"): a guest without a phone. That is
-- allowed only inside a group, next to the person who booked (who keeps a phone to call).
-- Recording their visit still needs a customer record (the desk creates one on arrival), so
-- the visit, payroll "làm" credit and history paths are unchanged.

ALTER TABLE app.appointment DROP CONSTRAINT appointment_check1;
ALTER TABLE app.appointment
  ADD CONSTRAINT appointment_customer_or_guest CHECK (
       (customer_id IS NOT NULL AND guest_name IS NULL AND guest_phone IS NULL)
    OR (customer_id IS NULL AND guest_name IS NOT NULL)),
  ADD CONSTRAINT appointment_guest_phone_or_group CHECK (
       customer_id IS NOT NULL OR guest_phone IS NOT NULL OR group_id IS NOT NULL);
