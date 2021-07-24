/*
  FISCO BCOS/rust-SDK is a rust client for FISCO BCOS2.0 (https://github.com/FISCO-BCOS/)
  FISCO BCOS/rust-SDK is free software: you can redistribute it and/or modify it under the
  terms of the MIT License as published by the Free Software Foundation. This project is
  distributed in the hope that it will be useful, but WITHOUT ANY WARRANTY; without even
  the implied warranty of MERCHANTABILITY or FITNESS FOR A PARTICULAR PURPOSE.
  @author: kentzhang
  @date: 2021-07
*/

/*
此文件的部分实现参考了https://docs.rs/ethabi，https://github.com/rust-ethereum/ethabi
该项目采用Apache许可
由于其部分实现是私有的，所以在这里参考原代码进行修改
*/
#![allow(
    clippy::unreadable_literal,
    clippy::upper_case_acronyms,
    dead_code,
    non_camel_case_types,
    non_snake_case,
    non_upper_case_globals,
    overflowing_literals,
    unused_imports,
    unused_variables,
    unused_assignments
)]
use std::collections::HashMap;
use std::fs::File;

use crate::bcossdk::kisserror::{KissErrKind, KissError};
use anyhow::anyhow;
use ethabi::param_type::Writer;
use ethabi::{
    param_type::ParamType,
    token::{LenientTokenizer, StrictTokenizer, Token, Tokenizer},
    Bytes, Contract, Event, Function, Hash, Log as ReceiptLog, Param, RawLog,
};
use hex_literal::hex;
use keccak_hash::keccak;
use rustc_hex::ToHex;
use serde_json::Value as JsonValue;

use crate::bcossdk::commonhash::{CommonHash, HashType};
use crate::bcossdk::event_utils;
use std::path::PathBuf;
use std::str::FromStr;

#[derive(Clone, Debug)]
pub struct ContractABI {
    pub abi_file: String,
    pub contract: Contract,
    pub event_hash_map: HashMap<Hash, Event>,
    pub func_selector_map: HashMap<Vec<u8>, Function>,
    pub hashtype: HashType,
}

#[derive(Clone, Debug)]
pub struct function_input {
    pub func: Function,
    pub input: Vec<Token>,
}

impl ContractABI {
    pub fn new_by_name(
        contractname: &str,
        path: &str,
        hashtype: &HashType,
    ) -> Result<ContractABI, KissError> {
        let mut pb = PathBuf::from_str(path).unwrap();
        pb = pb.join(format!("{}.abi", contractname));
        ContractABI::new(pb.to_str().unwrap(), hashtype)
    }
    ///指定文件加载abi定义，注意hashtype，可指定多种hash算法，一定要和当前的节点或sdk实例一致
    pub fn new(filename: &str, hashtype: &HashType) -> Result<ContractABI, KissError> {
        printlnex!("try load contract file {}", filename);
        let contractfile_result = File::open(filename);
        match &contractfile_result {
            Err(e) => {
                return kisserr!(
                    KissErrKind::EFormat,
                    "load abi file {} error : {:?}",
                    filename,
                    e
                );
            }
            _ => {}
        }
        let contractfile = contractfile_result.unwrap();
        let contact_result = Contract::load(contractfile);
        match contact_result {
            Err(e) => {
                return kisserr!(KissErrKind::EFormat, "parse abi file error: {:?}", e);
            }
            _ => {}
        }
        let contract_obj = contact_result.unwrap();
        let mut contract = ContractABI {
            abi_file: String::from(filename),
            contract: contract_obj,
            event_hash_map: HashMap::new(),
            func_selector_map: HashMap::new(),
            hashtype: hashtype.clone(),
        };
        contract.map_event_to_hash();
        contract.map_function_to_selector();

        Ok(contract)
    }

    ///这个方法算hash时，会带上返回值类型一起算，如 set(String):(int256)
    pub fn function_signature_to_4byte_selector(func: &Function) -> Option<Vec<u8>> {
        let signature = func.signature().replace(" ", "");
        //println!("4bytes: {}",signature);
        let selectorhash = keccak(signature.as_bytes());
        let selector = selectorhash.as_bytes()[0..4].to_vec();
        Option::from(selector)
    }
    ///标准算短签名的方法，不会在最后附加返回值
    pub fn function_short_signature(func: &Function, hashtype: &HashType) -> Vec<u8> {
        let params: Vec<ParamType> = func.inputs.iter().map(|p| p.kind.clone()).collect();
        let types = params
            .iter()
            .map(Writer::write)
            .collect::<Vec<String>>()
            .join(",");
        let signature = format!("{}({})", func.name, types);
        //println!("short {:?}",signature);
        let data: Vec<u8> = From::from(signature.as_str());
        let hashres = CommonHash::hash(&data, hashtype);
        hashres.as_slice()[0..4].to_vec()
    }

    pub fn map_function_to_selector(&mut self) {
        for funcs in self.contract.functions.iter() {
            for func in funcs.1 {
                //let sig = ContractABI::function_signature_to_4byte_selector(func).unwrap();
                let shortsig = ContractABI::function_short_signature(func, &self.hashtype);
                self.func_selector_map
                    .insert(shortsig.clone(), func.clone());
                printlnex!(
                    "func selector {:?} for {:?}",
                    hex::encode(shortsig.as_slice()),
                    func.signature()
                );
            }
        }
    }

    pub fn find_func_by_selector(&self, selector: &Vec<u8>) -> Option<&Function> {
        let getresult = self.func_selector_map.get(&*selector);
        //println!("find_func_by_selector {:?}",getresult);
        getresult
    }

    pub fn map_event_to_hash(&mut self) {
        let contract = &self.contract;
        for (index, val) in self.contract.events.iter().enumerate() {
            let event = val.1.get(0).unwrap();
            event.signature();
            let hash = event_utils::event_signature(&event, &self.hashtype);
            self.event_hash_map.insert(hash, event.clone());
            //println!("event hash {} ,event {:?}", hex::encode(hash), event);
        }
    }
    pub fn find_event_by_hash(&self, key: Hash) -> Option<&Event> {
        let getresult = self.event_hash_map.get(&key);
        getresult
    }

    pub fn find_event_by_hashstring(&self, hashstr: String) -> Option<&Event> {
        let prefixsave = hashstr.trim_start_matches("0x");
        let key: Hash = prefixsave.parse().unwrap();
        let getresult = self.event_hash_map.get(&key);
        getresult
    }

    pub fn find_function(&self, name_or_signature: &str) -> anyhow::Result<Function> {
        let contract = &self.contract;
        let params_start = name_or_signature.find('(');
        match params_start {
            Some(params_start) => {
                let name = &name_or_signature[..params_start];

                contract
                    .functions_by_name(name)?
                    .iter()
                    .find(|f| f.signature() == name_or_signature)
                    .cloned()
                    .ok_or_else(|| anyhow!("invalid function signature `{}`", name_or_signature))
            }

            None => {
                let functions = contract.functions_by_name(name_or_signature)?;
                match functions.len() {
                    0 => unreachable!(),
                    1 => Ok(functions[0].clone()),
                    _ => Err(anyhow!(
					"More than one function found for name `{}`, try providing the full signature",
					name_or_signature
				)),
                }
            }
        }
    }

    ///将构造函数的参数编码后追加到code后面。如果code为空，相当于只编码参数。
    /// 此版本的实现中，是单独编码参数，然后从文件中加载code的hex串，进行部署
    pub fn encode_construtor_input(
        &self,
        code: Bytes,
        values: &[String],
        lenient: bool,
    ) -> anyhow::Result<String> {
        let cons = self.contract.constructor().unwrap();
        let params: Vec<_> = cons
            .inputs
            .iter()
            .map(|param| param.kind.clone())
            .zip(values.iter().map(|v| v as &str))
            .collect();
        let tokens = self.collect_tokens(&params, lenient)?;
        //println!("encode input tokens:{:?}",tokens);
        //println!("{}",hex::encode(&code));
        let result = cons.encode_input(code, &tokens)?;
        //println!("{}",hex::encode(&result));
        Ok(hex::encode(&result))
    }

    pub fn encode_func_input_tokens(
        func: &Function,
        tokens: &[Token],
        hashtype: &HashType,
    ) -> Result<Bytes, KissError> {
        let params: Vec<ParamType> = func.inputs.iter().map(|p| p.kind.clone()).collect();

        if !Token::types_check(tokens, &params) {
            return kisserr!(KissErrKind::EFormat, "types_check");
        }
        let signed = ContractABI::function_short_signature(func, hashtype).to_vec();
        let encoded = ethabi::encode(tokens);
        Ok(signed.into_iter().chain(encoded.into_iter()).collect())
    }

    pub fn encode_function_input(
        &self,
        name_or_signature: &str,
        values: &[String],
        lenient: bool,
    ) -> anyhow::Result<String, KissError> {
        let functionres = self.find_function(name_or_signature);
        let function = match functionres {
            Ok(f) => f,
            Err(e) => {
                return kisserr!(
                    KissErrKind::EFormat,
                    "find_function {} {:?}",
                    name_or_signature,
                    e
                );
            }
        };
        // let sig = ContractABI::function_signature_to_4byte_selector(&function).unwrap();
        //let shortsig  = ContractABI::function_short_signature(&function);
        //println!("encode_function_input ,sig is {:?} : {:?}",hex::encode(shortsig),function);

        let params: Vec<_> = function
            .inputs
            .iter()
            .map(|param| param.kind.clone())
            .zip(values.iter().map(|v| v as &str))
            .collect();
        //println!("encode input params:{:?}",params);
        let tokensres = self.collect_tokens(&params, lenient);
        let tokens = match tokensres {
            Ok(t) => t,
            Err(e) => {
                return kisserr!(
                    KissErrKind::EFormat,
                    "make tokens from params error {:?}",
                    e
                );
            }
        };
        //println!("encode input tokens:{:?}",tokens);
        //function.encode_input(&tokens);
        let res = ContractABI::encode_func_input_tokens(&function, &tokens, &self.hashtype);
        let txinput = match res {
            Ok(s) => s,
            Err(e) => {
                return kisserr!(
                    KissErrKind::EFormat,
                    "abi encode error {} {:?}",
                    name_or_signature,
                    e
                );
            }
        };
        Ok(hex::encode(&txinput))
    }

    pub fn collect_tokens(
        &self,
        params: &[(ParamType, &str)],
        lenient: bool,
    ) -> anyhow::Result<Vec<Token>> {
        params
            .iter()
            .map(|&(ref param, value)| match lenient {
                true => LenientTokenizer::tokenize(param, value),
                false => StrictTokenizer::tokenize(param, value),
            })
            .collect::<anyhow::Result<_, _>>()
            .map_err(From::from)
    }

    /*解析合约函数的的返回，传入名字，比如“set"*/
    pub fn decode_output_byname(
        &self,
        name_or_signature: &str,
        data: &str,
    ) -> anyhow::Result<Vec<Token>> {
        let function = self.find_function(name_or_signature)?;
        printlnex!("decode_call_output_byname {:?}", function);
        self.decode_function_output(&function, data)
    }

    /*解析合约函数的的返回*/
    pub fn decode_function_output(
        &self,
        function: &Function,
        datainput: &str,
    ) -> anyhow::Result<Vec<Token>> {
        let data = datainput.trim_start_matches("0x");
        let data: Vec<u8> = hex::decode(&data)?;
        let tokens = function.decode_output(&data)?;
        Ok(tokens)
    }

    pub fn decode_input_for_tx(&self, txinput: &str) -> anyhow::Result<function_input, KissError> {
        let txinput_trim = txinput.trim_start_matches("0x");
        let selectorstr = &txinput_trim[0..8];
        let selector = hex::decode(selectorstr).unwrap();
        let funopt = self.find_func_by_selector(&selector);
        match funopt {
            Some(fun) => {
                let data = &txinput_trim[8..];
                let decoderesult = fun.decode_input(hex::decode(data).unwrap().as_slice());
                match decoderesult {
                    Ok(input) => {
                        let parse_result = function_input {
                            func: fun.clone(),
                            input: input,
                        };
                        Ok(parse_result)
                    }
                    Err(e) => {
                        kisserr!(KissErrKind::EFormat, "parse function error {:?}", e)
                    }
                }
            }
            None => {
                //println!("not found func");
                kisserr!(KissErrKind::EFormat, "function not found")
            }
        }
    }

    pub fn convert_json_to_rawlog(&self, logitem: &JsonValue) -> Option<RawLog> {
        //println!("log  {:?}", logitem);
        let logdata = &logitem["data"];
        let topics = &logitem["topics"];
        //println!("logdata {}", logdata);
        //println!("parse_receipt_logs topics {}", topics);
        let mut rawlogtopic: Vec<Hash> = Vec::new();
        for (pos, e) in topics.as_array().unwrap().iter().enumerate() {
            //println!("iter in topics {:?}", e);
            let v = e.as_str().unwrap();
            //println!("{:?}", v);
            let hexv: Hash = v.trim_start_matches("0x").parse().unwrap();
            rawlogtopic.push(hexv);
        }
        //println!("{:?}", rawlogtopic);

        let rawlogitem = ethabi::RawLog {
            topics: rawlogtopic,
            data: hex::decode(logdata.as_str().unwrap().trim_start_matches("0x")).unwrap(),
        };
        //println!("{:?}", rawlogitem);
        Option::from(rawlogitem)
    }

    pub fn parse_receipt_logs(&self, log_list: &JsonValue) -> Result<Vec<ReceiptLog>, KissError> {
        //let abi_path = "contracts/HelloWorld.abi";
        //let contract = Contract_abi::new(abi_path);
        //println!("total to parse logs {}",&log_list.as_array().unwrap().len());
        let mut loglistresult: Vec<ReceiptLog> = Vec::new();
        for (pos, e) in log_list.as_array().unwrap().iter().enumerate() {
            printlnex!(
                "\nparse log {}-------------------------------------------------):",
                pos
            );

            let rawlog = self.convert_json_to_rawlog(e).unwrap();
            printlnex!("the raw log : {:?}", rawlog);
            let eventabi = self.find_event_by_hash(rawlog.topics[0]);
            printlnex!("find_event_by_hash : {:?}", eventabi);
            match eventabi {
                Some(e) => {
                    //println!("event abi is {:?}",e);
                    //println!("the raw log: {:?}",rawlog);
                    let parse_result = event_utils::parse_log(&e, rawlog, &self.hashtype);
                    printlnex!("log parse result: eventname:{}: {:?}", e.name, parse_result);
                    match parse_result {
                        Ok(log) => {
                            loglistresult.push(log);
                        }

                        Err(e) => {
                            return kisserr!(KissErrKind::EFormat, "parse log error {:?}", e);
                        }
                    }
                }
                None => {
                    return kisserr!(KissErrKind::Error, "event not found for {:?}", e);
                }
            }
        } //for
        Ok(loglistresult)
    }
}

//----------------------------------------------------------------
pub fn parse_log(log_abi: &str, data: &str) {
    let abi_path = "contracts/HelloWorld.abi";
    let contract = ContractABI::new(abi_path, &HashType::WEDPR_KECCAK);
}

pub fn test_parse_log() {
    let abi_path = "contracts/HelloWorld.abi";
    let contract_result = ContractABI::new(abi_path, &HashType::WEDPR_KECCAK);
    let logdata = "000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000103132333437383930616263656667686500000000000000000000000000000000";
    let contract = contract_result.unwrap();
    for (pos, e) in contract.contract.events.iter().enumerate() {
        println!("event {:?}", e);
    }
    let onset_events = contract.contract.events_by_name("onset").unwrap();

    for (pos, e) in onset_events.iter().enumerate() {
        println!("Element at position {}: {:?}", pos, e);
        println!(
            "event signature(topic) {:?}",
            e.signature().as_bytes().to_hex()
        );
        let rawlog = ethabi::RawLog {
            topics: vec![hex!("afb180742c1292ea5d67c4f6d51283ecb11e49f8389f4539bef82135d689e118").into()],
            data: hex!("000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000103132333437383930616263656667686500000000000000000000000000000000")
                .into(),
        };
        println!("{:?}", rawlog);
        let ebyhash = contract.find_event_by_hashstring(String::from(
            "afb180742c1292ea5d67c4f6d51283ecb11e49f8389f4539bef82135d689e118",
        ));
        match ebyhash {
            Some(event) => {
                let result = e.parse_log(rawlog.clone());
                let log = result.ok().unwrap();
                println!("log  by  hash is : {:?}", log);
            }
            None => {
                println!("not fond event by hash");
            }
        }

        let result = e.parse_log(rawlog);
        let log = result.ok().unwrap();
        println!("log is : {:?}", log);
    }
}

pub fn test_contract() {
    let abi_path = "contracts/HelloWorld.abi";
    let contract = ContractABI::new(abi_path, &HashType::WEDPR_KECCAK);
    match &contract {
        Ok(c) => {
            println!("contract is {:?}", c);
        }
        Err(e) => {
            println!("{:?}", e);
            return;
        }
    }
    let params: [String; 1] = [String::from("12347890abc")];
    let hellores = contract
        .unwrap()
        .encode_function_input("set", &params, false)
        .ok();
    println!("contract  set rawdata :{}", hellores.unwrap().as_str());
    test_parse_log();
    test_parse_tx_input();
}

pub fn test_parse_tx_input() {
    let txinput = "4ed3885e000000000000000000000000000000000000000000000000000000000000002000000000000000000000000000000000000000000000000000000000000000103132333437383930616263656667686500000000000000000000000000000000";
    let abi_path = "contracts/HelloWorld.abi";
    let contract = ContractABI::new(abi_path, &HashType::WEDPR_KECCAK);
    println!("{:?}", &contract);
    let funopt = contract.unwrap().decode_input_for_tx(txinput);
    match funopt {
        Ok(input_result) => {
            println!("{:?}", input_result);
            println!("function is {:?}", input_result.func);
            let parseresult = &input_result.input;
            println!("parseresult : {:?}", parseresult);
            for t in parseresult.iter() {
                println!("{}", input_result.func.name);
                println!("{}", t.to_string());
            }
        }
        Err(e) => {
            println!("not found func {:?}", e);
        }
    }
}
