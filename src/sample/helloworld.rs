#![allow(
    clippy::unreadable_literal,
    clippy::upper_case_acronyms,
    dead_code,
    non_camel_case_types,
    non_snake_case,
    non_upper_case_globals,
    overflowing_literals,
    unused_imports,
    unused_results,
    unused_variables
)]
use std::time::Duration;
use crate::bcossdk::bcossdk::BcosSDK;
use crate::bcossdk::contractabi::ContractABI;
use crate::bcossdk::kisserror::KissError;
use crate::bcossdk::bcossdkquery;
use std::thread;
use crate::bcossdk::bcossdkquery::json_hextoint;
use crate::bcossdk::contracthistory;
use crate::bcossdk::contracthistory::ContractHistory;
use crate::bcossdk::channelpack::ChannelPack;
use crate::bcossdk::liteutils::datetime_str;
use crate::console::console_utils;
pub fn demo_deploy_helloworld(bcossdk: &mut BcosSDK) -> Result<String,KissError>
{
    let binpath = format!("{}/HelloWorld.bin",bcossdk.config.contract.contractpath);
    println!("Contract Bin file {}",&binpath);
    let v = bcossdk.deploy_file(binpath.as_str(), "");
    println!("request response {:?}", v);
    let response = v.unwrap();
    let txhash = response["result"].as_str().unwrap();
    //thread::sleep(Duration::from_secs(1));
    let recepitresult = bcossdk.try_getTransactionReceipt( txhash,3,false);
    //println!("receipt {:?}",recepitresult);
    let receipt = recepitresult.unwrap();
    console_utils::display_transaction_receipt(&receipt,&Option::None,&bcossdk.config);
    let addr:String = receipt["result"]["contractAddress"].as_str().unwrap().to_string();
    let blocknum = json_hextoint(&receipt["result"]["blockNumber"]).unwrap();
    println!("deploy contract on block {}",blocknum);
    let chfile = format!("{}/contracthistory.toml",bcossdk.config.contract.contractpath);
    let res = ContractHistory::save_to_file(chfile.as_str(),"HelloWorld",addr.as_str(),blocknum as u32);
    Ok(addr)

}


//---------------------------------------------------------
pub fn demo(configfile:&str)
{
    let mut bcossdk = BcosSDK::new_from_config(configfile).unwrap();
    let block_limit = bcossdk.getBlockLimit();
    println!("block limit {:?}",block_limit);

    println!("\n>>>>>>>>>>>>>>>>>>demo deploy contract");
    let newaddr = demo_deploy_helloworld(&mut bcossdk).unwrap();
    println!("new addr {}",&newaddr);

    let contract = ContractABI::new_by_name("HelloWorld",
                                            bcossdk.config.contract.contractpath.as_str(),
                                            &bcossdk.hashtype).unwrap();
    //let to_address = String::from("882be29b2d5ac85d6c476fa3fd5f0cae4b4585cc");
    let to_address = newaddr;
    //let params: [String; 1] = [String::from("this is a test string for helloworld")];
    let paramsvec = vec!(format!("Test string for helloworld: {}",datetime_str()));
    println!("\n>>>>>>>>>>>>>>>>>>>>demo helloworld set");
    let txres = bcossdk.send_raw_transaction(&contract, &to_address, &"set".to_string(), paramsvec.as_slice());
    println!("send_raw_transaction result {:?}", txres);

    let response = txres.unwrap();
    println!("response[\"result\"] {:?}",response);
    let txhash = response["result"].as_str().unwrap();

    thread::sleep(Duration::from_secs(1));

    println!("\n>>>>>>>>>>>>>>>>>>>demo helloworld getTransactionByHash");
    let txdata = bcossdk.getTransactionByHash(txhash).unwrap();
    let blocknum = bcossdkquery::json_hextoint(&txdata["result"]["blockNumber"]);
    let txinput = txdata["result"]["input"].as_str().unwrap();
    let inputdecode = contract.decode_input_for_tx(txinput).unwrap();
    println!("tx input :{:?}",inputdecode);

    println!("\n>>>>>>>>>>>>>>>>>>>>demo helloworld getTransactionReceipt");
    let recepitresult = bcossdk.try_getTransactionReceipt( txhash, 3,false);
    console_utils::display_transaction_receipt(&recepitresult.unwrap(),&Option::from(&contract),&bcossdk.config);

    let callvalue = bcossdk.call(&contract, &to_address, "get", &["".to_string()]).unwrap();
    let output = callvalue["result"]["output"].as_str().unwrap();

    println!("\n>>>>>>>>>>>>>>>>>>>>demo helloworld call get");
    let decodereuslt = contract.decode_output_byname("get", output);
    println!("get function output: {:?}",decodereuslt);


    println!("\n>>>>>>>>>>>>>>>>>>>>demo helloworld set and get proof");
    let params =vec!(String::from("the test 2"));
    let txres = bcossdk.sendRawTransactionAndGetProof(&contract, &to_address, "set",params.as_slice());
    println!("send_raw_transaction result {:?}", txres);
        let response = txres.unwrap();
    println!("response[\"result\"] {:?}",response);
    let txhash = response["result"].as_str().unwrap();

    thread::sleep(Duration::from_secs(1));

    println!("\n>>>>>>>>>>>>>>>>>>>demo helloworld getTransactionByHash with proof");
    let txdata = bcossdk.getTransactionByHashWithProof(txhash).unwrap();
    println!("getTransactionReceiptByHashWithProof : {:?}",&txdata);
    let res = contract.decode_input_for_tx(txdata["result"]["transaction"]["input"].as_str().unwrap());
    println!("decode tx input : {:?}",res);

    println!("\n>>>>>>>>>>>>>>>>>>>demo helloworld getTransactionReceiptByHashWithProof with proof");
    let receipt = bcossdk.getTransactionReceiptByHashWithProof (txhash);
    println!("getTransactionReceiptByHashWithProof {:?}",receipt);

    if bcossdk.netclient.channel_client.channelpackpool.len()>0 {
        println!("channelpackpool size is {}",&bcossdk.netclient.channel_client.channelpackpool.len());
        for pack in &bcossdk.netclient.channel_client.channelpackpool
        {
            println!("{:?}",pack.detail());
        }

    }
    println!("NodeVersion:{:?}",bcossdk.getNodeVersion());
    bcossdk.finish();

}