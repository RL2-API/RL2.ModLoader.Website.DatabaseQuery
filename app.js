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
	console.log("Start refetching the mod list...");
	const result = await db.execute(`
		SELECT info.name, info.author, info.icon_src, info.short_desc
		FROM info INNER JOIN versions ON info.name == versions.name 
		GROUP BY info.name 
		ORDER BY MAX(versions.id) DESC
	`);
	mod_list_data = result.rows;
	console.log("Finalize refetching the mod list...");
}

async function get_mod_data() {
	console.log("Start refetching info about specific mods...");
	const info =  await db.execute(`
		SELECT DISTINCT info.name, info.long_desc, info.icon_src, info.author
		FROM info
	`);

	const links = await db.execute(`
		SELECT name, link, version, changelog
		FROM versions 
		ORDER BY version DESC
	`);

	for (const element of mod_list_data) {
		const name = element.name;
		const result = {
			mod_info: info.rows.filter(row => row.name === name)[0],
			versions: links.rows.filter(row => row.name === name),
		};
		mod_data[name.toLowerCase()] = result;
	}
	console.log("Start refetching info about specific mods...");
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
		if (refetchable) {
			await refetch_mod_list();
			refetchable = false;
		}
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
