ATTACH DATABASE ':memory:' AS map;

CREATE TABLE map.templates (
	name TEXT,
	id TEXT,
	UNIQUE(name, id)
);

CREATE TABLE map.dependencies (
	parent_id TEXT,
	dependency_id TEXT,
	UNIQUE(parent_id, dependency_id)
);