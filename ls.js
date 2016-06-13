'use strict'

var statvfs = require('./statvfs')
var args = require('cli.args')()
var fs = require('fs');

function stat(filename) {
    return new Promise( (resolve, reject) => {
        fs.lstat(filename, (err, result) => {
            if(err) {
                return reject(err)
            }
            return resolve(result)
        })
    })
}

const promises = args.nonOpt.map( (filename) => {
    return Promise.resolve({filename})
    .then( (r) => statvfs(filename).then( (vfs) => Object.assign({}, r, {vfs}) ) )
    .then( (vfs) => stat(filename).then( (stat) => Object.assign({}, vfs, {stat}) ) )
    //.then( (result) => (console.log(result), result))
    .catch( (err) => console.error(err) )
})

Promise.all(promises)
.then( (results) => {
    console.log(require('util').inspect(results, {depth: null, colors: true}))
})