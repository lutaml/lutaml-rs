// Wait for actual asynchronous browser completion through Chrome's public CDP
// interface; dump-dom's virtual-time deadline can precede WebCrypto completion.
import { spawn } from 'node:child_process';
import { once } from 'node:events';
export async function runBrowser(chrome, profile, url) {
  const child = spawn(chrome, ['--headless=new', '--disable-gpu', '--no-first-run', `--user-data-dir=${profile}`, '--remote-debugging-address=127.0.0.1', '--remote-debugging-port=0', 'about:blank']);
  let socket, deadline, stderr = '';
  const pending = new Map(), events = new Map();
  try {
    return await Promise.race([
      new Promise((_, reject) => { deadline = setTimeout(() => reject(Error(`browser smoke timeout: ${stderr}`)), 60000); }),
      (async () => {
        const endpoint = await new Promise((resolve, reject) => {
          child.once('error', reject);
          child.once('close', code => reject(Error(`browser exited ${code}: ${stderr}`)));
          child.stdout.resume();
          child.stderr.on('data', data => {
            stderr += data;
            const match = stderr.match(/DevTools listening on (ws:\/\/\S+)/);
            if (match) resolve(match[1]);
          });
        });
        socket = new WebSocket(endpoint);
        await once(socket, 'open');
        let nextId = 0;
        socket.addEventListener('message', ({ data }) => {
          const message = JSON.parse(data);
          if (message.id) {
            const task = pending.get(message.id);
            if (task) {
              pending.delete(message.id);
              if (message.error) task.reject(Error(JSON.stringify(message.error)));
              else task.resolve(message.result);
            }
          } else {
            const key = `${message.sessionId}:${message.method}`;
            const resolve = events.get(key);
            if (resolve) { events.delete(key); resolve(message.params); }
          }
        });
        socket.addEventListener('close', () => {
          for (const task of pending.values()) task.reject(Error('browser connection closed'));
        });
        const send = (method, params = {}, sessionId) => new Promise((resolve, reject) => {
          const id = ++nextId; pending.set(id, { resolve, reject });
          socket.send(JSON.stringify({ id, method, params, ...(sessionId && { sessionId }) }));
        });
        const { targetId } = await send('Target.createTarget', { url: 'about:blank' });
        const { sessionId } = await send('Target.attachToTarget', { targetId, flatten: true });
        await send('Page.enable', {}, sessionId);
        const loaded = new Promise(resolve => events.set(`${sessionId}:Page.loadEventFired`, resolve));
        await send('Page.navigate', { url }, sessionId); await loaded;
        const result = await send('Runtime.evaluate', {
          expression: `new Promise(resolve => {
            const timer = setInterval(() => {
              const text = document.querySelector('#result')?.textContent;
              if (text?.startsWith('PASS ') || text?.startsWith('FAIL ')) { clearInterval(timer); resolve(text); }
            }, 25);
          })`, awaitPromise: true, returnByValue: true,
        }, sessionId);
        if (result.exceptionDetails || !result.result.value?.startsWith('PASS ')) throw Error(`browser smoke failed: ${JSON.stringify(result)}`);
        return result.result.value.slice(5);
      })(),
    ]);
  } finally {
    clearTimeout(deadline); socket?.close();
    if (child.exitCode === null && child.signalCode === null) {
      const closed = once(child, 'close'); child.kill('SIGTERM'); await closed;
    }
  }
}
