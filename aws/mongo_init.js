db.createUser({
  user: 'relay',
  pwd: '{{password}}',
  roles: [
    {
      role: 'readWrite',
      db: 'relay',
    },
  ],
});
