/** @type {import('next').NextConfig} */
const nextConfig = {
  async rewrites() {
    return [
      {
        source: '/api/admin/:path*',
        destination: 'http://127.0.0.1:9090/api/v1/:path*',
      },
    ];
  },
};

module.exports = nextConfig;
