import styled from "styled-components";

export const Wizard = styled.div`
  margin: 20px 0;
`;

export const Dialog = styled.div`
  padding: 30px;
  background: #333;
  box-shadow: 0 0 5px #242424;
  border-radius: 5px;
  margin-bottom: 40px;
`;

export const StepHeader = styled.header`
  font-size: 18px;
  display: flex;
  flex-direction: column;
  justify-content: center;
  align-items: center;
  margin-bottom: 40px;

  &:last-child {
    margin-bottom: 0;
  }
`;

export const StepNumber = styled.span`
  display: inline-block;
  width: 18px;
  height: 18px;
  font-size: 12px;
  padding-left: 1px;
  display: flex;
  justify-content: center;
  align-items: center;
  border-radius: 50px;
  border: 1px solid #aaa;
  margin-bottom: 10px;
`;

export const Environments = styled.div`
  display: flex;
  flex-direction: row;
`;

export const Environment = styled.div`
  width: 50%;
  padding: 0 20px;
  display: flex;
  flex-direction: column;
  align-items: center;

  h3 {
    font-size: 24px;
    font-weight: normal;
    text-align: center;
    margin: 0 0 5px 0;
    display: none;
  }

  p {
    margin: 0 0 20px 0;
    text-align: center;
  }
`;

export const Dropdown = styled.div`
  width: 350px;
  font-size: 24px;
  text-align: center;
`;

export const Separator = styled.hr`
  width: 1px;
  height: auto;
  background: #555;
  border: none;
  box-shadow: none;
`;
