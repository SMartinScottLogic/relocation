'use strict'

var statvfs = require('./statvfs')
var args = require('cli.args')()

args.nonOpt.forEach( (arg) => {
  var vfs = statvfs(arg);
  console.log(arg, vfs)
})

