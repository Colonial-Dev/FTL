DROP TABLE IF EXISTS attributes;
DROP TABLE IF EXISTS routes;
DROP TABLE IF EXISTS dependencies;
DROP TABLE IF EXISTS output;

-- Tables with columns referenced by foreign key constraints
-- need to be dropped *last*, or cryptic "table does not exist" 
-- errors will be generated.
DROP TABLE IF EXISTS pages;
DROP TABLE IF EXISTS revisions;
DROP TABLE IF EXISTS revision_files;
DROP TABLE IF EXISTS input_files;

VACUUM;