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

async function refetch_mod_list() {
	const result = await db.execute("SELECT name, description, author, icon_src FROM shortinfo GROUP BY name ORDER BY id DESC");
	mod_list_data = result.rows;
}

async function get_mod_data() {
	for (const element of mod_list_data) {
		const info =  await db.execute(`
			SELECT DISTINCT shortinfo.name, longinfo.description, longinfo.changelog, shortinfo.icon_src, shortinfo.author
			FROM longinfo INNER JOIN shortinfo ON longinfo.name = shortinfo.name 
			WHERE shortinfo.name LIKE '${element.name}'`
		);

		const links = await db.execute(`
			SELECT link, version
			FROM shortinfo 
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

setInterval(() => { refetchable = true; }, 3 * 60 * 1000); // Refetch mod list every 3 minutes

app.get('/mod-list', async (req, res) => {
	if (refetchable) {
		await refetch_mod_list();
		await get_mod_data();
		refetchable = false;
	}
	res.set
	res.send(mod_list_data);
});

app.get('/mod/:name', async (req, res) => {
	if (refetchable) {
		await refetch_mod_list();
		await get_mod_data();
		refetchable = false;
	}
	
	const result = mod_data[req.params.name.toLowerCase()];

	res.send(result);
});

app.get('/', (req, res) => res.sendFile(process.cwd() + '/index.html'));

app.listen(port, () => {
	console.log("Server started");
});
