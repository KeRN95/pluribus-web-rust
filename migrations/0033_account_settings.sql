-- Adding a phone number or an e-mail address to an account ("Cilësimet", web/settings.rs): the
-- code goes to the new contact and says which account it is for, the one signed in when it was
-- asked for, which the contact joins once the code comes back.
ALTER TABLE login_codes ADD COLUMN user_id uuid REFERENCES users(id) ON DELETE CASCADE;
