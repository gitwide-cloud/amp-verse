// Optional live-protocol benchmark, invoked explicitly by the desktop smoke tool.
import {Client} from '../src/client.js'
let count=0;const client=new Client(process.argv[2],'',()=>count++);
await client.connect();await new Promise(resolve=>setTimeout(resolve,12000));client.close();
console.log(`Companion client received ${count} actual state updates`);if(count<10)process.exitCode=1;
