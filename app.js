import { createClient } from "@libsql/client";
import express from 'express';

const app = express();
const port = 4000;

app.use((req, res, next) => {
    res.append('Access-Control-Allow-Origin', ['*']);
    res.append('Access-Control-Allow-Methods', 'GET,PUT,POST,DELETE');
    res.append('Access-Control-Allow-Headers', 'Content-Type');
    next();
});

const db = createClient({
	url: process.env.DB_URL,
	authToken: process.env.AUTH,
});

let mod_list_data = [];
let mod_data = {};
let refetchable = true;
let mods_refetchable = true;

async function refetch_mod_list() {
	const result = await db.execute(`
		SELECT info.name, info.author, info.icon_src, info.short_desc
		FROM info INNER JOIN versions ON info.name == versions.name 
		GROUP BY info.name 
		ORDER BY MAX(versions.id) DESC
	`);
	mod_list_data = result.rows;
}

async function get_mod_data() {
	for (const element of mod_list_data) {
		const info =  await db.execute(`
			SELECT DISTINCT info.name, info.long_desc, info.icon_src, info.author
			FROM info INNER JOIN versions ON info.name = versions.name 
			WHERE info.name LIKE '${element.name}'`
		);

		const links = await db.execute(`
			SELECT link, version, changelog
			FROM versions 
			WHERE name LIKE '${element.name}'
			ORDER BY version DESC
		`);

		const result = {
			mod_info: info.rows,
			versions: links.rows,
		};
		mod_data[element.name.toLowerCase()] = result;
	}
}

setInterval(() => { 
	refetchable = true; 
	mods_refetchable = true;
}, 3 * 60 * 1000); // Refetch mod list every 3 minutes

app.get('/mod-list', async (req, res) => {
	if (refetchable) {
		await refetch_mod_list();
		refetchable = false;
	}
	
	res.send(mod_list_data);
});

app.get('/mod/:name', async (req, res) => {
	const name = req.params.name.toLowerCase();
	if (mods_refetchable) {
		await get_mod_data();
		mods_refetchable = false;
	}
	
	const result = mod_data[name];
	res.send(result);
});

app.get('/', (req, res) => res.sendFile(process.cwd() + '/index.html'));

app.listen(port, () => {
	console.log("Server started");
});