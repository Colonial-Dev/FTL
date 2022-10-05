ATTACH DATABASE ':memory:' AS map;

CREATE TABLE IF NOT EXISTS map.templates (
	name TEXT,
	id TEXT,
	UNIQUE(name, id)
);

CREATE TABLE IF NOT EXISTS map.dependencies (
	parent_id TEXT,
	dependency_id TEXT,
	UNIQUE(parent_id, dependency_id)
);