'use strict'

var statvfs = require('./statvfs')
var args = require('cli.args')()
var fs = require('fs');
var path = require('path')

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

function get_traits( stat ) {
    return ['File','Directory','BlockDevice','CharacterDevice','SymbolicLink','FIFO','Socket']
    .reduce( (traits, trait) => (traits[trait] = stat[`is${trait}`](),traits), {})
}

function map( filestat) {
    console.log(filestat.root, filestat.filename)
    return { [filestat.filename]: true}
}

function handle_single_path( root, pathname ) {
    return new Promise( (resolve, reject) => {
        fs.readdir( pathname, (err, entries) => {
            if(err) {
                return reject(err)
            }
            const promises = entries.map( (entry) => {
                const filename = path.join( pathname, entry)
                return Promise.resolve({root, filename})
                //.then( (r) => statvfs(filename).then( (vfs) => Object.assign({}, r, {vfs}) ) )
                .then( (results) => stat(filename).then( (stat) => Object.assign({}, vfs, {stat} ) ) )
                .then( (results) => Object.assign({}, results, {traits: get_traits(results.stat)}) )
                .then( (results) => results.stat.isDirectory() ? handle_single_path( root, results.filename ).then(()=>results) : results)
                .then( map )
            })
            return resolve(Promise.all(promises))
        })
    })
}

const promises = args.nonOpt.map( (filename) => handle_single_path(filename, filename) )

Promise.all(promises)
.then( (results) => {
    console.log(require('util').inspect(results, {depth: null, colors: true}))
})