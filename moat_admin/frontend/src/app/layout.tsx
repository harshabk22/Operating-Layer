import React from 'react';
import './globals.css';
import ClientLayout from './ClientLayout';

export const metadata = {
  title: 'MoatAgeex Control Plane',
  description: 'Enterprise Administrative Dashboard for MoatAgeex Operating Layer',
};

export default function RootLayout({
  children,
}: {
  children: React.ReactNode;
}) {
  return (
    <html lang="en" className="dark">
      <head>
        <link
          href="https://fonts.googleapis.com/css2?family=Inter:wght@300;400;500;600;700&family=Geist+Mono:wght@300;400;500;600;700&family=Public+Sans:wght@300;400;500;600;700&display=swap"
          rel="stylesheet"
        />
      </head>
      <body>
        <ClientLayout>{children}</ClientLayout>
      </body>
    </html>
  );
}
