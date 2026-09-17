/**
 * Dev proxy: browser `/api/*` → Actix Commerce API.
 *
 * Override when `:8080` is already taken (e.g. another app on this host):
 *
 *   RUSTASHOP_API_PROXY=http://127.0.0.1:8081 npm start
 *   API_BIND=127.0.0.1:8081 make run-api
 *   RUSTASHOP_API_PROXY=http://127.0.0.1:8081 make shop-angular
 */
function apiTarget() {
  const fromProxy = process.env.RUSTASHOP_API_PROXY;
  if (fromProxy) {
    return fromProxy.replace(/\/$/, '');
  }
  const bind = process.env.RUSTASHOP_BIND || '127.0.0.1:8080';
  if (bind.startsWith('http://') || bind.startsWith('https://')) {
    return bind.replace(/\/$/, '');
  }
  return `http://${bind}`;
}

const target = apiTarget();

module.exports = {
  '/api': {
    target,
    secure: false,
    changeOrigin: true,
    ws: true,
    pathRewrite: {
      '^/api': '',
    },
    logLevel: 'warn',
  },
};
