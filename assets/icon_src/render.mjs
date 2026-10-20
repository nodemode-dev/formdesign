import { chromium } from 'playwright';
const [,, svgPath, outPath, size] = process.argv;
import fs from 'fs';
const svg = fs.readFileSync(svgPath,'utf8');
const s = parseInt(size);
const b = await chromium.launch(); const p = await b.newPage({viewport:{width:s,height:s}});
await p.setContent(`<html><body style="margin:0;background:transparent">${svg.replace('width="1024" height="1024"',`width="${s}" height="${s}"`)}</body></html>`);
await p.screenshot({path:outPath, omitBackground:true});
await b.close();
