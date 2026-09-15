ALTER TABLE applications ADD COLUMN iam_check_after INTEGER NOT NULL DEFAULT 0;
CREATE INDEX applications_iam_check ON applications(plane,iam_check_after);
